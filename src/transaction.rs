use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{AruError, IoContext, Result};

mod destination;
mod install;
mod staging;
mod standalone;
mod state_file;
use destination::{normalize_destination, validate_operations};

pub use standalone::{
    StandaloneDryRun, apply_standalone, apply_standalone_global, apply_standalone_prepared,
};

pub(crate) use standalone::{PreviewLock, lock_without_pending_journal};

pub const JOURNAL_FILE: &str = ".aru/transaction.toml";

#[derive(Debug)]
pub enum Content {
    File(Vec<u8>),
    Directory {
        source: PathBuf,
        expected_skill_digest: Option<String>,
    },
    Symlink(PathBuf),
    Absent,
}

#[derive(Debug)]
pub struct Operation {
    pub destination: PathBuf,
    pub content: Content,
}

impl Operation {
    pub fn file(path: impl Into<PathBuf>, bytes: Vec<u8>) -> Self {
        Self {
            destination: path.into(),
            content: Content::File(bytes),
        }
    }

    pub fn directory(path: impl Into<PathBuf>, source: impl Into<PathBuf>) -> Self {
        Self {
            destination: path.into(),
            content: Content::Directory {
                source: source.into(),
                expected_skill_digest: None,
            },
        }
    }

    pub fn skill_directory(
        path: impl Into<PathBuf>,
        source: impl Into<PathBuf>,
        expected_skill_digest: impl Into<String>,
    ) -> Self {
        Self {
            destination: path.into(),
            content: Content::Directory {
                source: source.into(),
                expected_skill_digest: Some(expected_skill_digest.into()),
            },
        }
    }

    pub fn symlink(path: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        Self {
            destination: path.into(),
            content: Content::Symlink(target.into()),
        }
    }

    pub fn remove(path: impl Into<PathBuf>) -> Self {
        Self {
            destination: path.into(),
            content: Content::Absent,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Journal {
    version: u32,
    phase: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    root: Option<String>,
    entries: Vec<JournalEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct JournalEntry {
    destination: String,
    stage: Option<String>,
    backup: Option<String>,
    old_digest: Option<String>,
    new_digest: Option<String>,
    applied: bool,
}

#[derive(Clone, Copy)]
enum PathMode<'a> {
    Project(&'a Path),
    Absolute,
}

pub struct ProjectLock {
    _standalone_file: standalone::GlobalLock,
    _legacy_file: Option<File>,
    _file: File,
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        standalone::unlock_files([Some(&self._file), self._legacy_file.as_ref()]);
        // GlobalLock then releases the shared operation and anchor locks.
    }
}

impl ProjectLock {
    pub fn acquire(project: &Path) -> Result<Self> {
        let (standalone_file, legacy_file, standalone_journal) =
            standalone::acquire_for_project(project)?;
        recover_standalone_if_needed_at(&standalone_journal)?;
        let aru = project.join(".aru");
        std::fs::create_dir_all(&aru).at(&aru)?;
        let path = aru.join("operation.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .at(&path)?;
        file.lock_exclusive().map_err(|error| {
            AruError::msg(format!("could not acquire project operation lock: {error}"))
        })?;
        Ok(Self {
            _standalone_file: standalone_file,
            _legacy_file: legacy_file,
            _file: file,
        })
    }
}

pub fn recover_if_needed(project: &Path) -> Result<bool> {
    recover_if_needed_at(project, &project.join(JOURNAL_FILE))
}

fn recover_if_needed_at(project: &Path, path: &Path) -> Result<bool> {
    recover_with_mode(PathMode::Project(project), path)
}

fn recover_standalone_if_needed_at(path: &Path) -> Result<bool> {
    let Some(mut journal) = read_journal(path)? else {
        return Ok(false);
    };
    let root = journal
        .root
        .as_deref()
        .map(decode_absolute_path)
        .transpose()?;
    let mode = root
        .as_deref()
        .map(PathMode::Project)
        .unwrap_or(PathMode::Absolute);
    recover_journal(mode, path, &mut journal)
}

fn recover_with_mode(mode: PathMode<'_>, path: &Path) -> Result<bool> {
    let Some(mut journal) = read_journal(path)? else {
        return Ok(false);
    };
    recover_journal(mode, path, &mut journal)
}

fn read_journal(path: &Path) -> Result<Option<Journal>> {
    let Some(text) = state_file::read(path)? else {
        return Ok(None);
    };
    let journal = toml::from_str(&text).map_err(|source| AruError::Toml {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(journal))
}

fn recover_journal(mode: PathMode<'_>, path: &Path, journal: &mut Journal) -> Result<bool> {
    if !matches!(journal.version, 1 | 2) {
        return Err(AruError::msg("unsupported transaction journal version"));
    }
    if journal.phase == "committed" {
        for entry in &journal.entries {
            let stored = decode_journal_path(journal.version, mode, &entry.destination)?;
            validate_destination(mode, &stored)?;
            validate_ancestors(mode, &stored)?;
            let destination = resolve_path(mode, &stored);
            if path_digest(&destination)? != entry.new_digest {
                return Err(AruError::msg(format!(
                    "cannot finish committed transaction: {} no longer matches its new digest",
                    entry.destination
                )));
            }
        }
    } else {
        rollback(mode, path, journal)?;
    }
    cleanup_journal_artifacts(mode, journal)?;
    std::fs::remove_file(path).at(path)?;
    sync_parent(path)?;
    Ok(true)
}

pub fn apply(project: &Path, operations: Vec<Operation>) -> Result<()> {
    apply_at(project, operations, &project.join(JOURNAL_FILE))
}

fn apply_at(project: &Path, operations: Vec<Operation>, journal_path: &Path) -> Result<()> {
    apply_with_mode(
        PathMode::Project(project),
        operations,
        journal_path,
        1,
        true,
    )
}

fn apply_standalone_at(
    project: &Path,
    operations: Vec<Operation>,
    journal_path: &Path,
    replace: bool,
) -> Result<()> {
    apply_with_mode(
        PathMode::Project(project),
        operations,
        journal_path,
        2,
        replace,
    )
}

fn apply_absolute_at(operations: Vec<Operation>, journal_path: &Path, replace: bool) -> Result<()> {
    apply_with_mode(PathMode::Absolute, operations, journal_path, 2, replace)
}

fn apply_with_mode(
    mode: PathMode<'_>,
    mut operations: Vec<Operation>,
    journal_path: &Path,
    journal_version: u32,
    replace: bool,
) -> Result<()> {
    if operations.is_empty() {
        return Ok(());
    }
    validate_operations(mode, &operations, journal_version)?;
    if let Some(parent) = journal_path.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    if matches!(mode, PathMode::Absolute) {
        for operation in &mut operations {
            operation.destination = normalize_destination(&operation.destination)?;
        }
    }
    operations.sort_by(|left, right| left.destination.cmp(&right.destination));

    let transaction_id = unique_suffix();
    let mut journal = Journal {
        version: journal_version,
        phase: "prepared".into(),
        root: match (journal_version, mode) {
            (2, PathMode::Project(project)) => Some(encode_absolute_path(project)?),
            _ => None,
        },
        entries: Vec::new(),
    };
    let mut staging = staging::Staging::default();
    let preparation = (|| -> Result<()> {
        for (index, operation) in operations.iter().enumerate() {
            validate_destination(mode, &operation.destination)?;
            let destination = resolve_path(mode, &operation.destination);
            let parent = destination
                .parent()
                .ok_or_else(|| AruError::msg("transaction destination has no parent"))?;
            validate_ancestors(mode, &operation.destination)?;
            staging.create_parents(parent)?;
            let stage = if matches!(&operation.content, Content::Absent) {
                None
            } else {
                Some(parent.join(format!(".aru-stage-{transaction_id}-{index}")))
            };
            if let Some(stage) = &stage {
                staging.paths.push(stage.clone());
                materialize_stage(stage, &operation.content)?;
                sync_tree(stage)?;
            }
            #[cfg(test)]
            install::run_hook(install::Event::Staged, &destination);
            if !replace && destination_exists(&destination) {
                return Err(AruError::msg(format!(
                    "collision: unmanaged entry appeared at {}",
                    destination.display()
                )));
            }
            let old_digest = if replace {
                path_digest(&destination)?
            } else {
                None
            };
            let backup = old_digest
                .as_ref()
                .map(|_| parent.join(format!(".aru-backup-{transaction_id}-{index}")));
            let new_digest = stage.as_deref().map(path_digest).transpose()?.flatten();
            journal.entries.push(JournalEntry {
                destination: encode_journal_path(journal_version, mode, &destination)?,
                stage: stage
                    .as_ref()
                    .map(|path| encode_journal_path(journal_version, mode, path))
                    .transpose()?,
                backup: backup
                    .as_ref()
                    .map(|path| encode_journal_path(journal_version, mode, path))
                    .transpose()?,
                old_digest,
                new_digest,
                applied: false,
            });
        }
        Ok(())
    })();
    if let Err(error) = preparation {
        return Err(staging.cleanup(error));
    }
    if let Err(error) = write_journal(journal_path, &journal) {
        if journal_path.exists() {
            return recover_after_error(mode, journal_path, error);
        }
        return Err(staging.cleanup(error));
    }
    journal.phase = "applying".into();
    if let Err(error) = write_journal(journal_path, &journal) {
        return recover_after_error(mode, journal_path, error);
    }

    for index in 0..journal.entries.len() {
        let step = (|| -> Result<()> {
            let destination_text = journal.entries[index].destination.clone();
            let backup_text = journal.entries[index].backup.clone();
            let stage_text = journal.entries[index].stage.clone();
            let destination_path = decode_journal_path(journal.version, mode, &destination_text)?;
            let destination = resolve_path(mode, &destination_path);
            validate_ancestors(mode, &destination_path)?;
            if let Some(backup) = backup_text {
                let backup = decode_journal_path(journal.version, mode, &backup)?;
                let backup = resolve_path(mode, &backup);
                if destination_exists(&destination) {
                    std::fs::rename(&destination, &backup).at(&destination)?;
                    sync_parent(&destination)?;
                }
            }
            if let Some(stage) = stage_text {
                let stage = decode_journal_path(journal.version, mode, &stage)?;
                let stage = resolve_path(mode, &stage);
                #[cfg(test)]
                install::run_hook(install::Event::Installing, &destination);
                if replace {
                    std::fs::rename(&stage, &destination).at(&destination)?;
                } else {
                    install::rename_no_replace(&stage, &destination)?;
                }
                sync_parent(&destination)?;
            }
            journal.entries[index].applied = true;
            write_journal(journal_path, &journal)
        })();
        if let Err(error) = step {
            return recover_after_error(mode, journal_path, error);
        }
        if failure_phase("ARU_TEST_CRASH_AFTER") == Some(index + 1) {
            return Err(AruError::msg(format!(
                "simulated crash after transaction phase {} (journal retained)",
                index + 1
            )));
        }
        if failure_phase("ARU_TEST_FAIL_AFTER") == Some(index + 1) {
            return recover_after_error(
                mode,
                journal_path,
                AruError::msg(format!(
                    "simulated apply failure after transaction phase {}",
                    index + 1
                )),
            );
        }
    }

    journal.phase = "committed".into();
    if let Err(error) = write_journal(journal_path, &journal) {
        return recover_after_error(mode, journal_path, error);
    }
    cleanup_journal_artifacts(mode, &journal)?;
    std::fs::remove_file(journal_path).at(journal_path)?;
    sync_parent(journal_path)?;
    Ok(())
}

fn recover_after_error(mode: PathMode<'_>, journal_path: &Path, original: AruError) -> Result<()> {
    match recover_with_mode(mode, journal_path) {
        Ok(_) => Err(original),
        Err(recovery) => Err(AruError::msg(format!(
            "{original}; rollback also failed: {recovery}"
        ))),
    }
}

fn rollback(mode: PathMode<'_>, journal_path: &Path, journal: &mut Journal) -> Result<()> {
    for index in (0..journal.entries.len()).rev() {
        let destination_text = journal.entries[index].destination.clone();
        let old_digest = journal.entries[index].old_digest.clone();
        let new_digest = journal.entries[index].new_digest.clone();
        let backup_text = journal.entries[index].backup.clone();
        let destination_path = decode_journal_path(journal.version, mode, &destination_text)?;
        validate_destination(mode, &destination_path)?;
        validate_ancestors(mode, &destination_path)?;
        let destination = resolve_path(mode, &destination_path);
        // A retained, verified stage with no backup proves this create never
        // reached its destination. Preserve even identical concurrent content.
        if !journal.entries[index].applied
            && backup_text.is_none()
            && let Some(stage) = journal.entries[index].stage.as_deref()
        {
            let stage = decode_journal_path(journal.version, mode, stage)?;
            let stage = resolve_path(mode, &stage);
            if destination_exists(&stage) && path_digest(&stage)? == new_digest {
                continue;
            }
        }
        let current = path_digest(&destination)?;
        let backup = backup_text
            .as_ref()
            .map(|value| decode_journal_path(journal.version, mode, value))
            .transpose()?
            .map(|value| resolve_path(mode, &value));
        let backup_digest = backup.as_deref().map(path_digest).transpose()?.flatten();

        let current_is_new = current == new_digest;
        let current_is_old = current == old_digest;
        let is_backup_only_window =
            current.is_none() && old_digest.is_some() && backup_digest == old_digest;
        if !current_is_new && !current_is_old && !is_backup_only_window {
            return Err(AruError::msg(format!(
                "cannot recover transaction: {destination_text} has unknown/manual content"
            )));
        }
        if current_is_old && backup_digest.is_none() {
            journal.entries[index].applied = false;
            write_journal(journal_path, journal)?;
            continue;
        }
        if let Some(expected_old) = &old_digest {
            if backup_digest.as_ref() != Some(expected_old) {
                return Err(AruError::msg(format!(
                    "cannot recover transaction: backup for {destination_text} does not match old digest"
                )));
            }
        } else if backup_digest.is_some() {
            return Err(AruError::msg(format!(
                "cannot recover transaction: unexpected backup for {destination_text}"
            )));
        }
        if current_is_new && destination_exists(&destination) {
            remove_any(&destination)?;
        }
        if let Some(backup) = backup
            && destination_exists(&backup)
        {
            std::fs::rename(&backup, &destination).at(&destination)?;
        }
        if path_digest(&destination)? != old_digest {
            return Err(AruError::msg(format!(
                "transaction recovery could not restore {destination_text}"
            )));
        }
        sync_parent(&destination)?;
        journal.entries[index].applied = false;
        write_journal(journal_path, journal)?;
    }
    Ok(())
}

fn materialize_stage(stage: &Path, content: &Content) -> Result<()> {
    if destination_exists(stage) {
        remove_any(stage)?;
    }
    match content {
        Content::File(bytes) => {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(stage)
                .at(stage)?;
            file.write_all(bytes).at(stage)?;
            file.sync_all().at(stage)?;
        }
        Content::Directory {
            source,
            expected_skill_digest,
        } => {
            copy_directory(source, stage)?;
            if let Some(expected) = expected_skill_digest
                && crate::skill::canonical_skill_digest(stage)? != *expected
            {
                return Err(AruError::msg(
                    "post-copy skill digest does not match the locked content",
                ));
            }
        }
        Content::Symlink(target) => create_symlink(target, stage)?,
        Content::Absent => {}
    }
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<()> {
    std::fs::create_dir(destination).at(destination)?;
    for item in WalkDir::new(source).follow_links(false).min_depth(1) {
        let item = item.map_err(|error| AruError::msg(format!("copy failed: {error}")))?;
        let relative = item
            .path()
            .strip_prefix(source)
            .map_err(|_| AruError::msg("copy source escaped its root"))?;
        let target = destination.join(relative);
        if item.file_type().is_dir() {
            std::fs::create_dir(&target).at(&target)?;
        } else if item.file_type().is_file() {
            std::fs::copy(item.path(), &target).at(&target)?;
            std::fs::set_permissions(
                &target,
                item.metadata()
                    .map_err(|error| {
                        AruError::msg(format!("could not inspect copy source: {error}"))
                    })?
                    .permissions(),
            )
            .at(&target)?;
        } else {
            return Err(AruError::msg(format!(
                "refusing to copy symlink or special entry {}",
                item.path().display()
            )));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).at(link)
}

#[cfg(windows)]
fn create_symlink(target: &Path, link: &Path) -> Result<()> {
    std::os::windows::fs::symlink_dir(target, link).at(link)
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_target: &Path, _link: &Path) -> Result<()> {
    Err(AruError::msg("directory symlinks are unsupported"))
}

fn validate_destination(mode: PathMode<'_>, destination: &Path) -> Result<()> {
    if destination.as_os_str().is_empty() {
        return Err(AruError::msg("transaction destination must not be empty"));
    }
    match mode {
        PathMode::Project(project) => {
            if destination.is_absolute()
                || destination
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(AruError::msg(
                    "transaction destination must be a safe project-relative path",
                ));
            }
            let canonical_project = project.canonicalize().at(project)?;
            if !canonical_project.is_dir() {
                return Err(AruError::msg("project root is not a directory"));
            }
        }
        PathMode::Absolute => {
            if !destination.is_absolute()
                || destination
                    .components()
                    .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
            {
                return Err(AruError::msg(
                    "global transaction destination must be a safe absolute path",
                ));
            }
        }
    }
    Ok(())
}

fn validate_ancestors(mode: PathMode<'_>, destination: &Path) -> Result<()> {
    let (mut current, canonical_project) = match mode {
        PathMode::Project(project) => (
            project.to_path_buf(),
            Some(project.canonicalize().at(project)?),
        ),
        PathMode::Absolute => (PathBuf::new(), None),
    };
    let components: Vec<_> = destination.components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        match mode {
            PathMode::Project(_) => {
                let Component::Normal(component) = component else {
                    return Err(AruError::msg("unsafe destination component"));
                };
                current.push(component);
            }
            PathMode::Absolute => current.push(component.as_os_str()),
        }
        if !current.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&current).at(&current)?;
        if metadata.file_type().is_symlink() {
            let resolved = current.canonicalize().at(&current)?;
            if !resolved.is_dir() {
                return Err(AruError::msg(format!(
                    "destination ancestor symlink does not resolve to a directory: {}",
                    current.display()
                )));
            }
            if canonical_project
                .as_ref()
                .is_some_and(|project| !resolved.starts_with(project))
            {
                return Err(AruError::msg(format!(
                    "destination ancestor symlink escapes project root: {}",
                    current.display()
                )));
            }
        } else if !metadata.is_dir() {
            return Err(AruError::msg(format!(
                "destination ancestor is not a directory: {}",
                current.display()
            )));
        }
    }
    Ok(())
}

pub fn path_digest(path: &Path) -> Result<Option<String>> {
    let Ok(root_metadata) = std::fs::symlink_metadata(path) else {
        return Ok(None);
    };
    let mut hasher = Sha256::new();
    hasher.update(b"aru-path-v1\0");
    if root_metadata.file_type().is_symlink() {
        let target = std::fs::read_link(path).at(path)?;
        hasher.update(b"link\0");
        hasher.update(target.to_string_lossy().as_bytes());
    } else if root_metadata.is_file() {
        hasher.update(b"file\0");
        hash_file(&mut hasher, path, &root_metadata)?;
    } else if root_metadata.is_dir() {
        hasher.update(b"dir\0");
        for item in WalkDir::new(path).follow_links(false).min_depth(1) {
            let item =
                item.map_err(|error| AruError::msg(format!("digest walk failed: {error}")))?;
            let relative = item
                .path()
                .strip_prefix(path)
                .map_err(|_| AruError::msg("digest path escaped root"))?;
            let relative = relative
                .to_str()
                .ok_or_else(|| AruError::msg("cannot digest non-UTF-8 path"))?
                .replace(std::path::MAIN_SEPARATOR, "/");
            hasher.update((relative.len() as u64).to_be_bytes());
            hasher.update(relative.as_bytes());
            if item.file_type().is_dir() {
                hasher.update(b"d");
            } else if item.file_type().is_file() {
                hasher.update(b"f");
                let metadata = item.metadata().map_err(|error| {
                    AruError::msg(format!("could not inspect digest entry: {error}"))
                })?;
                hash_file(&mut hasher, item.path(), &metadata)?;
            } else if item.file_type().is_symlink() {
                hasher.update(b"l");
                hasher.update(
                    std::fs::read_link(item.path())
                        .at(item.path())?
                        .to_string_lossy()
                        .as_bytes(),
                );
            } else {
                return Err(AruError::msg("cannot digest special filesystem entry"));
            }
        }
    } else {
        return Err(AruError::msg("cannot digest special filesystem entry"));
    }
    Ok(Some(format!("sha256:{}", hex::encode(hasher.finalize()))))
}

fn hash_file(hasher: &mut Sha256, path: &Path, metadata: &std::fs::Metadata) -> Result<()> {
    hasher.update(metadata.len().to_be_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        hasher.update([u8::from(metadata.permissions().mode() & 0o111 != 0)]);
    }
    #[cfg(not(unix))]
    hasher.update([0]);
    let mut file = File::open(path).at(path)?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            AruError::msg(format!("could not digest {}: {error}", path.display()))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

fn write_journal(path: &Path, journal: &Journal) -> Result<()> {
    let body = toml::to_string_pretty(journal)
        .map_err(|error| AruError::msg(format!("could not serialize journal: {error}")))?;
    state_file::write_atomic(path, &body)
}

fn cleanup_journal_artifacts(mode: PathMode<'_>, journal: &Journal) -> Result<()> {
    for entry in &journal.entries {
        for stored in [entry.stage.as_ref(), entry.backup.as_ref()]
            .into_iter()
            .flatten()
        {
            let stored = decode_journal_path(journal.version, mode, stored)?;
            let path = resolve_path(mode, &stored);
            if destination_exists(&path) {
                remove_any(&path)?;
            }
        }
    }
    Ok(())
}

fn remove_any(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path).at(path)?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        std::fs::remove_dir_all(path).at(path)
    } else {
        std::fs::remove_file(path).at(path)
    }
}

fn destination_exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn resolve_path(mode: PathMode<'_>, path: &Path) -> PathBuf {
    match mode {
        PathMode::Project(project) => project.join(path),
        PathMode::Absolute => path.to_path_buf(),
    }
}

fn encode_journal_path(version: u32, mode: PathMode<'_>, path: &Path) -> Result<String> {
    match mode {
        PathMode::Project(project) => path
            .strip_prefix(project)
            .map_err(|_| AruError::msg("transaction artifact escaped project"))?
            .to_str()
            .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
            .ok_or_else(|| AruError::msg("transaction path is not UTF-8")),
        PathMode::Absolute if version == 1 => path
            .to_str()
            .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
            .ok_or_else(|| AruError::msg("transaction path is not UTF-8")),
        PathMode::Absolute => encode_absolute_path(path),
    }
}

fn decode_journal_path(version: u32, mode: PathMode<'_>, stored: &str) -> Result<PathBuf> {
    match mode {
        PathMode::Project(_) => Ok(PathBuf::from(stored)),
        PathMode::Absolute if version == 1 => Ok(PathBuf::from(stored)),
        PathMode::Absolute => decode_absolute_path(stored),
    }
}

fn encode_absolute_path(path: &Path) -> Result<String> {
    if !path.is_absolute() {
        return Err(AruError::msg("standalone journal root must be absolute"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(format!("unix:{}", hex::encode(path.as_os_str().as_bytes())))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let bytes = path
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        Ok(format!("windows:{}", hex::encode(bytes)))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let value = path
            .to_str()
            .ok_or_else(|| AruError::msg("standalone journal path is not UTF-8"))?;
        Ok(format!("utf8:{}", hex::encode(value.as_bytes())))
    }
}

fn decode_absolute_path(stored: &str) -> Result<PathBuf> {
    let (encoding, encoded) = stored
        .split_once(':')
        .ok_or_else(|| AruError::msg("invalid encoded standalone journal path"))?;
    #[cfg(unix)]
    if encoding != "unix" {
        return Err(AruError::msg(
            "unsupported standalone journal path encoding",
        ));
    }
    #[cfg(windows)]
    if encoding != "windows" {
        return Err(AruError::msg(
            "unsupported standalone journal path encoding",
        ));
    }
    #[cfg(not(any(unix, windows)))]
    if encoding != "utf8" {
        return Err(AruError::msg(
            "unsupported standalone journal path encoding",
        ));
    }
    let bytes = hex::decode(encoded)
        .map_err(|_| AruError::msg("invalid encoded standalone journal path"))?;
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    };
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        if bytes.len() % 2 != 0 {
            return Err(AruError::msg("invalid encoded standalone journal path"));
        }
        let wide = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        PathBuf::from(std::ffi::OsString::from_wide(&wide))
    };
    #[cfg(not(any(unix, windows)))]
    let path = PathBuf::from(
        String::from_utf8(bytes)
            .map_err(|_| AruError::msg("invalid encoded standalone journal path"))?,
    );
    if !path.is_absolute() {
        return Err(AruError::msg(
            "encoded standalone journal path must be absolute",
        ));
    }
    Ok(path)
}

#[cfg(not(test))]
fn failure_phase(_variable: &str) -> Option<usize> {
    None
}

#[cfg(test)]
thread_local! {
    static TEST_FAIL_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
    static TEST_CRASH_AFTER: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
fn failure_phase(variable: &str) -> Option<usize> {
    match variable {
        "ARU_TEST_FAIL_AFTER" => TEST_FAIL_AFTER.get(),
        "ARU_TEST_CRASH_AFTER" => TEST_CRASH_AFTER.get(),
        _ => None,
    }
}

#[cfg(test)]
fn set_failure_phase(variable: &str, phase: Option<usize>) {
    match variable {
        "ARU_TEST_FAIL_AFTER" => TEST_FAIL_AFTER.set(phase),
        "ARU_TEST_CRASH_AFTER" => TEST_CRASH_AFTER.set(phase),
        _ => panic!("unknown transaction test failure variable"),
    }
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    format!(
        "{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn sync_tree(path: &Path) -> Result<()> {
    if path.is_file() {
        File::open(path).at(path)?.sync_all().at(path)?;
    } else if path.is_dir() {
        for item in WalkDir::new(path).contents_first(true) {
            let item = item.map_err(|error| AruError::msg(format!("sync walk failed: {error}")))?;
            if item.file_type().is_file() {
                File::open(item.path())
                    .at(item.path())?
                    .sync_all()
                    .at(item.path())?;
            }
        }
    }
    sync_parent(path)
}

fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    if let Some(parent) = path.parent() {
        File::open(parent).at(parent)?.sync_all().at(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
