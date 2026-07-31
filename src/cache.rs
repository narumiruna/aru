use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::digest::sha256_bytes;
use crate::error::{AruError, IoContext, Result};
use crate::source::git::{self, GitSource};

#[derive(Debug)]
pub struct Cache {
    root: PathBuf,
    fallback: Option<PathBuf>,
    invalidated_fallback: std::sync::Mutex<std::collections::BTreeSet<(String, String)>>,
    _ephemeral: Option<tempfile::TempDir>,
}

impl Cache {
    pub fn project(project: &Path) -> Self {
        Self {
            root: project.join(".aru/cache"),
            fallback: None,
            invalidated_fallback: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            _ephemeral: None,
        }
    }

    pub fn ephemeral() -> Result<Self> {
        Self::ephemeral_with_fallback(None)
    }

    pub fn ephemeral_for_project(project: &Path) -> Result<Self> {
        Self::ephemeral_with_fallback(Some(project.join(".aru/cache")))
    }

    fn ephemeral_with_fallback(fallback: Option<PathBuf>) -> Result<Self> {
        let temporary = tempfile::tempdir()
            .map_err(|error| AruError::msg(format!("could not create dry-run cache: {error}")))?;
        let root = temporary.path().join("cache");
        Ok(Self {
            root,
            fallback,
            invalidated_fallback: std::sync::Mutex::new(std::collections::BTreeSet::new()),
            _ephemeral: Some(temporary),
        })
    }

    pub fn checkout(&self, source: &GitSource, revision: &str) -> Result<PathBuf> {
        self.checkout_with_policy(source, revision, false)
    }

    pub fn checkout_with_policy(
        &self,
        source: &GitSource,
        revision: &str,
        offline: bool,
    ) -> Result<PathBuf> {
        let source_hash = source_hash(&source.identity);
        if let Some(fallback) = &self.fallback {
            let invalidated = self
                .invalidated_fallback
                .lock()
                .map_err(|_| AruError::msg("dry-run cache invalidation lock was poisoned"))?;
            if !invalidated.contains(&(source_hash.clone(), revision.to_owned())) {
                let shard = fallback.join("git").join(&source_hash).join(revision);
                let marker = shard.join(".complete");
                let content = shard.join("content");
                if marker.is_file() && content.is_dir() {
                    return Ok(content);
                }
            }
        }
        let source_root = self.root.join("git").join(&source_hash);
        std::fs::create_dir_all(&source_root).at(&source_root)?;
        let lock_path = source_root.join(format!(".{revision}.lock"));
        let lock = open_lock(&lock_path)?;
        lock.lock_exclusive()
            .map_err(|error| AruError::msg(format!("could not lock cache shard: {error}")))?;
        cleanup_incomplete(&source_root)?;

        let shard = source_root.join(revision);
        let marker = shard.join(".complete");
        let content = shard.join("content");
        if marker.is_file() && content.is_dir() {
            return Ok(content);
        }
        if offline && !source.is_local() {
            return Err(AruError::msg(format!(
                "offline mode cannot fetch uncached Git source {} at {revision}",
                source.identity
            )));
        }
        if shard.exists() {
            remove_any(&shard)?;
        }

        let staging = source_root.join(format!(
            ".incomplete-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        let staging_content = staging.join("content");
        std::fs::create_dir_all(&staging).at(&staging)?;
        let resolved = match git::checkout_exact(source, revision, &staging_content) {
            Ok(resolved) => resolved,
            Err(error) => {
                let _ = remove_any(&staging);
                return Err(error);
            }
        };
        if resolved != revision.to_ascii_lowercase() {
            let _ = remove_any(&staging);
            return Err(AruError::msg(format!(
                "Git checkout resolved {resolved}, but lock requires {revision}"
            )));
        }
        std::fs::write(
            staging.join(".complete"),
            format!("source={}\nrevision={}\n", source.identity, revision),
        )
        .at(staging.join(".complete"))?;
        std::fs::rename(&staging, &shard).at(&shard)?;
        Ok(content)
    }

    pub fn invalidate(&self, source: &GitSource, revision: &str) -> Result<()> {
        let hash = source_hash(&source.identity);
        if self.fallback.is_some() {
            self.invalidated_fallback
                .lock()
                .map_err(|_| AruError::msg("dry-run cache invalidation lock was poisoned"))?
                .insert((hash.clone(), revision.to_owned()));
        }
        let source_root = self.root.join("git").join(hash);
        let lock_path = source_root.join(format!(".{revision}.lock"));
        let lock = open_lock(&lock_path)?;
        lock.lock_exclusive()
            .map_err(|error| AruError::msg(format!("could not lock cache shard: {error}")))?;
        let shard = source_root.join(revision);
        if shard.exists() {
            remove_any(&shard)?;
        }
        Ok(())
    }

    pub fn garbage_collect(&self, referenced: &[(String, String)]) -> Result<()> {
        let git_root = self.root.join("git");
        if !git_root.is_dir() {
            return Ok(());
        }
        let retained: std::collections::BTreeSet<_> = referenced
            .iter()
            .map(|(identity, revision)| (source_hash(identity), revision.clone()))
            .collect();
        for source in std::fs::read_dir(&git_root).at(&git_root)? {
            let source = source.at(&git_root)?;
            if !source.file_type().at(source.path())?.is_dir() {
                continue;
            }
            let hash = source.file_name().to_string_lossy().into_owned();
            for shard in std::fs::read_dir(source.path()).at(source.path())? {
                let shard = shard.at(source.path())?;
                let name = shard.file_name().to_string_lossy().into_owned();
                if shard.file_type().at(shard.path())?.is_dir()
                    && !name.starts_with(".incomplete-")
                    && !retained.contains(&(hash.clone(), name.clone()))
                {
                    let lock_path = source.path().join(format!(".{name}.lock"));
                    let lock = open_lock(&lock_path)?;
                    lock.lock_exclusive().map_err(|error| {
                        AruError::msg(format!(
                            "could not lock cache shard for collection: {error}"
                        ))
                    })?;
                    if shard.path().exists() && !retained.contains(&(hash.clone(), name.clone())) {
                        remove_any(&shard.path())?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn source_hash(identity: &str) -> String {
    sha256_bytes(identity.as_bytes())
        .strip_prefix("sha256:")
        .unwrap()
        .to_owned()
}

fn open_lock(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .at(path)
}

fn cleanup_incomplete(root: &Path) -> Result<()> {
    for item in std::fs::read_dir(root).at(root)? {
        let item = item.at(root)?;
        if item
            .file_name()
            .to_string_lossy()
            .starts_with(".incomplete-")
        {
            remove_any(&item.path())?;
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

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(repository: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(repository)
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().into()
    }

    #[test]
    fn concurrent_fetches_land_one_complete_immutable_shard() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        let project = temporary.path().join("project");
        std::fs::create_dir(&repository).unwrap();
        std::fs::create_dir(&project).unwrap();
        git(&repository, &["init", "--quiet"]);
        git(&repository, &["config", "user.email", "cache@example.com"]);
        git(&repository, &["config", "user.name", "cache test"]);
        git(&repository, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repository.join("file"), "content").unwrap();
        git(&repository, &["add", "file"]);
        git(&repository, &["commit", "--quiet", "-m", "initial"]);
        let revision = git(&repository, &["rev-parse", "HEAD"]);
        let source = crate::source::git::canonicalize(temporary.path(), "repository").unwrap();

        let source_root = project
            .join(".aru/cache/git")
            .join(source_hash(&source.identity));
        std::fs::create_dir_all(source_root.join(".incomplete-dead")).unwrap();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let project = project.clone();
                let source = source.clone();
                let revision = revision.clone();
                std::thread::spawn(move || {
                    Cache::project(&project)
                        .checkout(&source, &revision)
                        .unwrap()
                })
            })
            .collect();
        let paths: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert!(paths.iter().all(|path| path == &paths[0]));
        assert_eq!(std::fs::read(paths[0].join("file")).unwrap(), b"content");
        assert!(!source_root.join(".incomplete-dead").exists());
        let completed = std::fs::read_dir(&source_root)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .count();
        assert_eq!(completed, 1);
    }
}
