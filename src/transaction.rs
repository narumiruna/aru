use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::error::{AruError, IoContext, Result};

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

pub struct ProjectLock {
    _file: File,
}

impl ProjectLock {
    pub fn acquire(project: &Path) -> Result<Self> {
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
        Ok(Self { _file: file })
    }
}

pub fn recover_if_needed(project: &Path) -> Result<bool> {
    let path = project.join(JOURNAL_FILE);
    if !path.exists() {
        return Ok(false);
    }
    let text = std::fs::read_to_string(&path).at(&path)?;
    let mut journal: Journal = toml::from_str(&text).map_err(|source| AruError::Toml {
        path: path.clone(),
        source,
    })?;
    if journal.version != 1 {
        return Err(AruError::msg("unsupported transaction journal version"));
    }
    if journal.phase == "committed" {
        for entry in &journal.entries {
            let destination = project.join(&entry.destination);
            validate_destination(project, Path::new(&entry.destination))?;
            validate_ancestors(project, Path::new(&entry.destination))?;
            if path_digest(&destination)? != entry.new_digest {
                return Err(AruError::msg(format!(
                    "cannot finish committed transaction: {} no longer matches its new digest",
                    entry.destination
                )));
            }
        }
    } else {
        rollback(project, &mut journal)?;
    }
    cleanup_journal_artifacts(project, &journal)?;
    std::fs::remove_file(&path).at(&path)?;
    sync_parent(&path)?;
    Ok(true)
}

pub fn apply(project: &Path, mut operations: Vec<Operation>) -> Result<()> {
    if operations.is_empty() {
        return Ok(());
    }
    let aru_directory = project.join(".aru");
    std::fs::create_dir_all(&aru_directory).at(&aru_directory)?;
    operations.sort_by(|left, right| left.destination.cmp(&right.destination));
    for pair in operations.windows(2) {
        if pair[0].destination == pair[1].destination {
            return Err(AruError::msg(format!(
                "transaction contains duplicate destination {}",
                pair[0].destination.display()
            )));
        }
    }

    let transaction_id = unique_suffix();
    let mut journal = Journal {
        version: 1,
        phase: "prepared".into(),
        entries: Vec::new(),
    };
    let mut staged_paths = Vec::new();
    let preparation = (|| -> Result<()> {
        for (index, operation) in operations.iter().enumerate() {
            validate_destination(project, &operation.destination)?;
            let destination = project.join(&operation.destination);
            let parent = destination
                .parent()
                .ok_or_else(|| AruError::msg("transaction destination has no parent"))?;
            validate_ancestors(project, &operation.destination)?;
            std::fs::create_dir_all(parent).at(parent)?;
            let stage = if matches!(&operation.content, Content::Absent) {
                None
            } else {
                Some(parent.join(format!(".aru-stage-{transaction_id}-{index}")))
            };
            let backup = if destination_exists(&destination) {
                Some(parent.join(format!(".aru-backup-{transaction_id}-{index}")))
            } else {
                None
            };
            if let Some(stage) = &stage {
                staged_paths.push(stage.clone());
                materialize_stage(stage, &operation.content)?;
                sync_tree(stage)?;
            }
            let old_digest = path_digest(&destination)?;
            let new_digest = stage.as_deref().map(path_digest).transpose()?.flatten();
            journal.entries.push(JournalEntry {
                destination: relative_string(project, &destination)?,
                stage: stage
                    .as_ref()
                    .map(|path| relative_string(project, path))
                    .transpose()?,
                backup: backup
                    .as_ref()
                    .map(|path| relative_string(project, path))
                    .transpose()?,
                old_digest,
                new_digest,
                applied: false,
            });
        }
        Ok(())
    })();
    if let Err(error) = preparation {
        cleanup_paths(&staged_paths);
        return Err(error);
    }
    if let Err(error) = write_journal(project, &journal) {
        if project.join(JOURNAL_FILE).exists() {
            return recover_after_error(project, error);
        }
        cleanup_paths(&staged_paths);
        return Err(error);
    }
    journal.phase = "applying".into();
    if let Err(error) = write_journal(project, &journal) {
        return recover_after_error(project, error);
    }

    for index in 0..journal.entries.len() {
        let step = (|| -> Result<()> {
            let destination_text = journal.entries[index].destination.clone();
            let backup_text = journal.entries[index].backup.clone();
            let stage_text = journal.entries[index].stage.clone();
            let destination = project.join(&destination_text);
            validate_ancestors(project, Path::new(&destination_text))?;
            if let Some(backup) = backup_text {
                let backup = project.join(backup);
                if destination_exists(&destination) {
                    std::fs::rename(&destination, &backup).at(&destination)?;
                    sync_parent(&destination)?;
                }
            }
            if let Some(stage) = stage_text {
                let stage = project.join(stage);
                std::fs::rename(&stage, &destination).at(&destination)?;
                sync_parent(&destination)?;
            }
            journal.entries[index].applied = true;
            write_journal(project, &journal)
        })();
        if let Err(error) = step {
            return recover_after_error(project, error);
        }
        if failure_phase("ARU_TEST_CRASH_AFTER") == Some(index + 1) {
            return Err(AruError::msg(format!(
                "simulated crash after transaction phase {} (journal retained)",
                index + 1
            )));
        }
        if failure_phase("ARU_TEST_FAIL_AFTER") == Some(index + 1) {
            return recover_after_error(
                project,
                AruError::msg(format!(
                    "simulated apply failure after transaction phase {}",
                    index + 1
                )),
            );
        }
    }

    journal.phase = "committed".into();
    if let Err(error) = write_journal(project, &journal) {
        return recover_after_error(project, error);
    }
    cleanup_journal_artifacts(project, &journal)?;
    let journal_path = project.join(JOURNAL_FILE);
    std::fs::remove_file(&journal_path).at(&journal_path)?;
    sync_parent(&journal_path)?;
    Ok(())
}

fn recover_after_error(project: &Path, original: AruError) -> Result<()> {
    match recover_if_needed(project) {
        Ok(_) => Err(original),
        Err(recovery) => Err(AruError::msg(format!(
            "{original}; rollback also failed: {recovery}"
        ))),
    }
}

fn cleanup_paths(paths: &[PathBuf]) {
    for path in paths {
        if destination_exists(path) {
            let _ = remove_any(path);
        }
    }
}

fn rollback(project: &Path, journal: &mut Journal) -> Result<()> {
    for index in (0..journal.entries.len()).rev() {
        let destination_text = journal.entries[index].destination.clone();
        let old_digest = journal.entries[index].old_digest.clone();
        let new_digest = journal.entries[index].new_digest.clone();
        let backup_text = journal.entries[index].backup.clone();
        let destination = project.join(&destination_text);
        validate_destination(project, Path::new(&destination_text))?;
        validate_ancestors(project, Path::new(&destination_text))?;
        let current = path_digest(&destination)?;
        let backup = backup_text.as_ref().map(|value| project.join(value));
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
            write_journal(project, journal)?;
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
        write_journal(project, journal)?;
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

fn validate_destination(project: &Path, relative: &Path) -> Result<()> {
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(AruError::msg(
            "transaction destination must be project-relative",
        ));
    }
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AruError::msg("unsafe transaction destination"));
    }
    let canonical_project = project.canonicalize().at(project)?;
    if !canonical_project.is_dir() {
        return Err(AruError::msg("project root is not a directory"));
    }
    Ok(())
}

fn validate_ancestors(project: &Path, relative: &Path) -> Result<()> {
    let canonical_project = project.canonicalize().at(project)?;
    let mut current = project.to_path_buf();
    let components: Vec<_> = relative.components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let Component::Normal(component) = component else {
            return Err(AruError::msg("unsafe destination component"));
        };
        current.push(component);
        if !current.exists() {
            continue;
        }
        let metadata = std::fs::symlink_metadata(&current).at(&current)?;
        if metadata.file_type().is_symlink() {
            let resolved = current.canonicalize().at(&current)?;
            if !resolved.starts_with(&canonical_project) {
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
    std::io::copy(&mut file, hasher)
        .map_err(|error| AruError::msg(format!("could not digest {}: {error}", path.display())))?;
    Ok(())
}

fn write_journal(project: &Path, journal: &Journal) -> Result<()> {
    let path = project.join(JOURNAL_FILE);
    let temporary = path.with_extension("toml.tmp");
    let body = toml::to_string_pretty(journal)
        .map_err(|error| AruError::msg(format!("could not serialize journal: {error}")))?;
    {
        let mut file = File::create(&temporary).at(&temporary)?;
        file.write_all(body.as_bytes()).at(&temporary)?;
        file.sync_all().at(&temporary)?;
    }
    std::fs::rename(&temporary, &path).at(&path)?;
    sync_parent(&path)
}

fn cleanup_journal_artifacts(project: &Path, journal: &Journal) -> Result<()> {
    for entry in &journal.entries {
        for relative in [entry.stage.as_ref(), entry.backup.as_ref()]
            .into_iter()
            .flatten()
        {
            let path = project.join(relative);
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

fn relative_string(project: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(project)
        .map_err(|_| AruError::msg("transaction artifact escaped project"))?
        .to_str()
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| AruError::msg("transaction path is not UTF-8"))
}

fn failure_phase(variable: &str) -> Option<usize> {
    std::env::var(variable).ok()?.parse().ok()
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
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn failed_post_copy_verification_leaves_no_stage_or_destination() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: source\ndescription: Source\n---\n# Source\n",
        )
        .unwrap();
        let result = apply(
            project.path(),
            vec![Operation::skill_directory(
                "skills/source",
                &source,
                "sha256:not-the-content",
            )],
        );
        assert!(result.is_err());
        assert!(!project.path().join("skills/source").exists());
        assert!(!project.path().join(JOURNAL_FILE).exists());
        assert!(
            std::fs::read_dir(project.path().join("skills"))
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".aru-stage-"))
        );
    }

    #[test]
    fn failed_phase_rolls_back_all_destinations() {
        let _serial = ENV_LOCK.lock().unwrap();
        for phase in 1..=3 {
            let project = tempfile::tempdir().unwrap();
            for name in ["a", "b", "c"] {
                std::fs::write(project.path().join(name), format!("old-{name}")).unwrap();
            }
            // SAFETY: these serialized tests are the only code reading this test variable.
            unsafe { std::env::set_var("ARU_TEST_FAIL_AFTER", phase.to_string()) };
            let result = apply(
                project.path(),
                ["a", "b", "c"]
                    .into_iter()
                    .map(|name| Operation::file(name, format!("new-{name}").into_bytes()))
                    .collect(),
            );
            unsafe { std::env::remove_var("ARU_TEST_FAIL_AFTER") };
            assert!(result.is_err());
            for name in ["a", "b", "c"] {
                assert_eq!(
                    std::fs::read(project.path().join(name)).unwrap(),
                    format!("old-{name}").as_bytes()
                );
            }
            assert!(!project.path().join(JOURNAL_FILE).exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn mixed_file_directory_and_symlink_transaction_rolls_back() {
        let _serial = ENV_LOCK.lock().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("config"), "old-config").unwrap();
        let old_skill = project.path().join("skill");
        std::fs::create_dir(&old_skill).unwrap();
        std::fs::write(old_skill.join("old"), "old-skill").unwrap();
        std::os::unix::fs::symlink("old-target", project.path().join("link")).unwrap();
        let source = project.path().join("source");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(
            source.join("SKILL.md"),
            "---\nname: source\ndescription: Source\n---\n# New\n",
        )
        .unwrap();
        let digest = crate::skill::canonical_skill_digest(&source).unwrap();
        unsafe { std::env::set_var("ARU_TEST_FAIL_AFTER", "3") };
        let result = apply(
            project.path(),
            vec![
                Operation::file("config", b"new-config".to_vec()),
                Operation::skill_directory("skill", &source, digest),
                Operation::symlink("link", "new-target"),
            ],
        );
        unsafe { std::env::remove_var("ARU_TEST_FAIL_AFTER") };
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(project.path().join("config")).unwrap(),
            b"old-config"
        );
        assert_eq!(
            std::fs::read(project.path().join("skill/old")).unwrap(),
            b"old-skill"
        );
        assert_eq!(
            std::fs::read_link(project.path().join("link")).unwrap(),
            PathBuf::from("old-target")
        );
    }

    #[test]
    fn every_crash_phase_is_recovered_on_next_invocation() {
        let _serial = ENV_LOCK.lock().unwrap();
        for phase in 1..=3 {
            let project = tempfile::tempdir().unwrap();
            for name in ["a", "b", "c"] {
                std::fs::write(project.path().join(name), format!("old-{name}")).unwrap();
            }
            unsafe { std::env::set_var("ARU_TEST_CRASH_AFTER", phase.to_string()) };
            let result = apply(
                project.path(),
                ["a", "b", "c"]
                    .into_iter()
                    .map(|name| Operation::file(name, format!("new-{name}").into_bytes()))
                    .collect(),
            );
            unsafe { std::env::remove_var("ARU_TEST_CRASH_AFTER") };
            assert!(result.is_err());
            assert!(recover_if_needed(project.path()).unwrap());
            for name in ["a", "b", "c"] {
                assert_eq!(
                    std::fs::read(project.path().join(name)).unwrap(),
                    format!("old-{name}").as_bytes()
                );
            }
        }
    }

    #[test]
    fn recovery_stops_on_unknown_manual_content() {
        let _serial = ENV_LOCK.lock().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join("a"), "old").unwrap();
        unsafe { std::env::set_var("ARU_TEST_CRASH_AFTER", "1") };
        assert!(apply(project.path(), vec![Operation::file("a", b"new".to_vec())]).is_err());
        unsafe { std::env::remove_var("ARU_TEST_CRASH_AFTER") };
        std::fs::write(project.path().join("a"), "manual").unwrap();
        assert!(recover_if_needed(project.path()).is_err());
        assert_eq!(std::fs::read(project.path().join("a")).unwrap(), b"manual");
        assert!(project.path().join(JOURNAL_FILE).exists());
    }

    #[test]
    fn committed_journal_rolls_forward_and_cleans_backup() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".aru")).unwrap();
        std::fs::write(project.path().join("a"), "new").unwrap();
        std::fs::write(project.path().join(".backup"), "old").unwrap();
        let journal = Journal {
            version: 1,
            phase: "committed".into(),
            entries: vec![JournalEntry {
                destination: "a".into(),
                stage: None,
                backup: Some(".backup".into()),
                old_digest: Some("sha256:old".into()),
                new_digest: path_digest(&project.path().join("a")).unwrap(),
                applied: true,
            }],
        };
        write_journal(project.path(), &journal).unwrap();
        assert!(recover_if_needed(project.path()).unwrap());
        assert_eq!(std::fs::read(project.path().join("a")).unwrap(), b"new");
        assert!(!project.path().join(".backup").exists());
    }

    #[test]
    fn crash_between_backup_and_stage_rename_restores_old_content() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".aru")).unwrap();
        std::fs::write(project.path().join("a"), "old").unwrap();
        let old_digest = path_digest(&project.path().join("a")).unwrap();
        let backup = project.path().join(".backup");
        std::fs::rename(project.path().join("a"), &backup).unwrap();
        let journal = Journal {
            version: 1,
            phase: "applying".into(),
            entries: vec![JournalEntry {
                destination: "a".into(),
                stage: None,
                backup: Some(".backup".into()),
                old_digest,
                new_digest: Some("sha256:new".into()),
                applied: false,
            }],
        };
        write_journal(project.path(), &journal).unwrap();
        assert!(recover_if_needed(project.path()).unwrap());
        assert_eq!(std::fs::read(project.path().join("a")).unwrap(), b"old");
    }

    #[test]
    fn v1_transaction_fixture_has_stable_round_trip() {
        let fixture = include_str!("../tests/fixtures/contracts/transaction.toml");
        let journal: Journal = toml::from_str(fixture).unwrap();
        assert_eq!(toml::to_string_pretty(&journal).unwrap(), fixture);
    }

    #[cfg(unix)]
    #[test]
    fn escaping_parent_symlink_is_rejected_before_staging() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), project.path().join(".agents")).unwrap();
        let result = apply(
            project.path(),
            vec![Operation::file(".agents/skills/demo", b"unsafe".to_vec())],
        );
        assert!(result.is_err());
        assert!(!outside.path().join("skills").exists());
    }
}
