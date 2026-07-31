use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use flate2::{Compression, GzBuilder};
use tar::{Builder, EntryType, Header};

use crate::error::{AruError, IoContext, Result};

use super::{
    MAX_GRAPH_BYTES, MAX_PACKAGE_DEPTH, MAX_PACKAGE_ENTRIES, PackageManifest, portable_path,
    validate_portable_path,
};

const MAX_ARCHIVE_FILE_BYTES: u64 = 32 * 1024 * 1024;

pub struct ArchiveEntry {
    pub path: String,
    pub(crate) bytes: Vec<u8>,
    mode: u32,
}

pub struct PackageSnapshot {
    temporary: tempfile::TempDir,
}

impl PackageSnapshot {
    pub fn root(&self) -> &Path {
        self.temporary.path()
    }
}

pub struct ArchiveInput {
    pub manifest: PackageManifest,
    pub entries: Vec<ArchiveEntry>,
    pub dirty: bool,
}

pub fn collect(root: &Path, output: Option<&Path>, allow_dirty: bool) -> Result<ArchiveInput> {
    let repository = git_output(root, &["rev-parse", "--show-toplevel"])?;
    let repository = PathBuf::from(repository.trim());
    let canonical_root = std::fs::canonicalize(root).at(root)?;
    let canonical_repository = std::fs::canonicalize(&repository).at(&repository)?;
    if canonical_root != canonical_repository {
        return Err(AruError::msg(format!(
            "aru package root must be the Git repository root ({})",
            canonical_repository.display()
        )));
    }
    let manifest = PackageManifest::load(root)?;
    let dirty_records = git_bytes(
        root,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    let output_relative = output
        .and_then(|path| path.strip_prefix(root).ok())
        .map(portable_path)
        .transpose()?;
    let dirty = dirty_records
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .any(|record| !generated_status_record(record, output_relative.as_deref()));
    if dirty && !allow_dirty {
        return Err(AruError::msg(
            "aru package working tree is dirty; commit changes or pass --allow-dirty",
        ));
    }

    let stage_modes = stage_modes(root)?;
    let inventory = git_bytes(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )?;
    let mut entries = Vec::new();
    let mut folded = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for path in inventory
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path)
            .map_err(|_| AruError::msg("package inventory path is not UTF-8"))?;
        validate_portable_path(path)?;
        if path.split('/').count() > MAX_PACKAGE_DEPTH {
            return Err(AruError::msg(format!(
                "package archive entry {path:?} exceeds maximum depth {MAX_PACKAGE_DEPTH}"
            )));
        }
        if generated_path(path, output_relative.as_deref()) {
            continue;
        }
        if entries.len() >= MAX_PACKAGE_ENTRIES {
            return Err(AruError::msg(format!(
                "aru package archive exceeds {MAX_PACKAGE_ENTRIES} entries"
            )));
        }
        let folded_path = path.to_lowercase();
        if !folded.insert(folded_path) {
            return Err(AruError::msg(format!(
                "aru package archive has a case-insensitive path collision at {path}"
            )));
        }
        for character in path.chars() {
            if crate::audit::hidden_unicode(character) {
                return Err(AruError::msg(format!(
                    "package archive path {path:?} contains hidden Unicode U+{:04X}",
                    character as u32
                )));
            }
        }
        let full_path = root.join(path);
        let metadata = match std::fs::symlink_metadata(&full_path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => continue,
            Err(source) => {
                return Err(AruError::Io {
                    path: full_path,
                    source,
                });
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(AruError::msg(format!(
                "package archive entry {path:?} must be a regular file, not a symlink or special file"
            )));
        }
        if metadata.len() > MAX_ARCHIVE_FILE_BYTES {
            return Err(AruError::msg(format!(
                "package archive entry {path:?} exceeds {MAX_ARCHIVE_FILE_BYTES} bytes"
            )));
        }
        total_bytes = total_bytes.saturating_add(metadata.len());
        if total_bytes > MAX_GRAPH_BYTES {
            return Err(AruError::msg(format!(
                "aru package archive exceeds {MAX_GRAPH_BYTES} bytes"
            )));
        }
        let bytes = std::fs::read(&full_path).at(&full_path)?;
        if let Ok(text) = std::str::from_utf8(&bytes) {
            for character in text.chars() {
                if crate::audit::hidden_unicode(character) {
                    return Err(AruError::msg(format!(
                        "package archive entry {path:?} contains hidden Unicode U+{:04X}",
                        character as u32
                    )));
                }
            }
        }
        let mode = if stage_modes.get(path).is_some_and(|mode| mode == "100755") {
            0o755
        } else {
            0o644
        };
        entries.push(ArchiveEntry {
            path: path.into(),
            bytes,
            mode,
        });
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    if !entries
        .iter()
        .any(|entry| entry.path == super::PACKAGE_MANIFEST_FILE)
    {
        return Err(AruError::msg(format!(
            "{} is not included by the Git package inventory",
            super::PACKAGE_MANIFEST_FILE
        )));
    }
    Ok(ArchiveInput {
        manifest,
        entries,
        dirty,
    })
}

pub fn snapshot(entries: &[ArchiveEntry]) -> Result<PackageSnapshot> {
    let temporary = tempfile::tempdir()
        .map_err(|error| AruError::msg(format!("could not create package snapshot: {error}")))?;
    for entry in entries {
        let destination = temporary.path().join(&entry.path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        std::fs::write(&destination, &entry.bytes).at(&destination)?;
    }
    git_bytes(temporary.path(), &["init", "--quiet"])?;
    let hooks = temporary.path().join(".git/aru-empty-hooks");
    std::fs::create_dir_all(&hooks).at(&hooks)?;
    let hooks = hooks
        .to_str()
        .ok_or_else(|| AruError::msg("package snapshot path is not UTF-8"))?;
    git_bytes(temporary.path(), &["config", "core.hooksPath", hooks])?;
    for arguments in [
        vec!["config", "user.email", "aru-package@localhost"],
        vec!["config", "user.name", "aru package"],
        vec!["config", "commit.gpgsign", "false"],
        vec!["add", "."],
        vec!["commit", "--quiet", "-m", "package snapshot"],
    ] {
        git_bytes(temporary.path(), &arguments)?;
    }
    Ok(PackageSnapshot { temporary })
}

pub fn bytes(entries: &[ArchiveEntry]) -> Result<Vec<u8>> {
    let mut compressed = Vec::new();
    {
        let encoder = GzBuilder::new()
            .mtime(0)
            .write(&mut compressed, Compression::default());
        let mut archive = Builder::new(encoder);
        for entry in entries {
            let mut header = Header::new_ustar();
            header.set_entry_type(EntryType::Regular);
            header.set_size(entry.bytes.len() as u64);
            header.set_mode(entry.mode);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_username("").map_err(|error| {
                AruError::msg(format!("could not normalize archive username: {error}"))
            })?;
            header.set_groupname("").map_err(|error| {
                AruError::msg(format!("could not normalize archive group name: {error}"))
            })?;
            header.set_cksum();
            archive
                .append_data(&mut header, &entry.path, entry.bytes.as_slice())
                .map_err(|error| {
                    AruError::msg(format!("could not build package archive: {error}"))
                })?;
        }
        let encoder = archive
            .into_inner()
            .map_err(|error| AruError::msg(format!("could not finish package archive: {error}")))?;
        encoder
            .finish()
            .map_err(|error| AruError::msg(format!("could not finish gzip archive: {error}")))?;
    }
    Ok(compressed)
}

pub fn validate_output_path(root: &Path, path: &Path) -> Result<()> {
    let Ok(relative) = path.strip_prefix(root) else {
        return Ok(());
    };
    let mut current = root.to_path_buf();
    for component in relative
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .components()
    {
        current.push(component.as_os_str());
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err(AruError::msg(format!(
                    "package archive output ancestor {} must be a real directory",
                    current.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(AruError::Io {
                    path: current,
                    source,
                });
            }
        }
    }
    if let Ok(metadata) = std::fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(AruError::msg(format!(
            "package archive output {} must be a regular file",
            path.display()
        )));
    }
    Ok(())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).at(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|source| AruError::Io {
        path: parent.into(),
        source,
    })?;
    temporary.write_all(bytes).at(temporary.path())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o644))
            .at(temporary.path())?;
    }
    temporary.as_file().sync_all().at(temporary.path())?;
    temporary.persist(path).map_err(|error| AruError::Io {
        path: path.into(),
        source: error.error,
    })?;
    Ok(())
}

fn stage_modes(root: &Path) -> Result<BTreeMap<String, String>> {
    let bytes = git_bytes(root, &["ls-files", "--stage", "-z"])?;
    let mut modes = BTreeMap::new();
    for record in bytes
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let record = std::str::from_utf8(record)
            .map_err(|_| AruError::msg("Git index path is not UTF-8"))?;
        let (metadata, path) = record
            .split_once('\t')
            .ok_or_else(|| AruError::msg("Git returned a malformed index record"))?;
        let mode = metadata
            .split_whitespace()
            .next()
            .ok_or_else(|| AruError::msg("Git returned a malformed index mode"))?;
        modes.insert(path.into(), mode.into());
    }
    Ok(modes)
}

fn generated_status_record(record: &[u8], output: Option<&str>) -> bool {
    if record.len() < 4 {
        return false;
    }
    std::str::from_utf8(&record[3..])
        .ok()
        .is_some_and(|path| generated_path(path, output))
}

fn generated_path(path: &str, output: Option<&str>) -> bool {
    path == ".aru"
        || path.starts_with(".aru/")
        || path.starts_with("target/aru-package/")
        || output.is_some_and(|output| path == output)
}

fn git_output(root: &Path, arguments: &[&str]) -> Result<String> {
    String::from_utf8(git_bytes(root, arguments)?)
        .map_err(|_| AruError::msg("Git output is not UTF-8"))
}

fn git_bytes(root: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| AruError::msg(format!("could not run Git: {error}")))?;
    if !output.status.success() {
        return Err(AruError::msg(format!(
            "Git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(output.stdout)
}
