//! Clean up preparation-only artifacts before a durable journal takes over.
use super::*;

#[derive(Default)]
pub(super) struct Staging {
    pub(super) paths: Vec<PathBuf>,
    parents: Vec<(PathBuf, std::fs::Metadata)>,
}

impl Staging {
    pub(super) fn create_parents(&mut self, parent: &Path) -> Result<()> {
        let mut missing = Vec::new();
        for path in parent.ancestors() {
            match path.symlink_metadata() {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(path.to_path_buf());
                }
                result => {
                    result.at(path)?;
                }
            }
        }
        for path in missing.into_iter().rev() {
            match std::fs::create_dir(&path) {
                Ok(()) => {
                    let metadata = path.symlink_metadata().at(&path)?;
                    self.parents.push((path, metadata));
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::AlreadyExists && path.is_dir() => {}
                result => result.at(&path)?,
            }
        }
        Ok(())
    }

    pub(super) fn cleanup(self, original: AruError) -> AruError {
        let mut failures = Vec::new();
        for path in self.paths.iter().rev() {
            if destination_exists(path)
                && let Err(error) = remove_any(path)
            {
                failures.push(error.to_string());
            }
        }
        for (path, created) in self.parents.iter().rev() {
            let metadata = match path.symlink_metadata() {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    failures.push(format!("{}: {error}", path.display()));
                    continue;
                }
            };
            if !same_directory(created, &metadata) {
                failures.push(format!(
                    "{} was replaced; preserved for review",
                    path.display()
                ));
                continue;
            }
            // Never recursively delete a parent: concurrent content belongs to
            // its creator, and pre-existing directories are never recorded.
            if let Err(error) = std::fs::remove_dir(path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                failures.push(format!("{}: {error}", path.display()));
            }
        }
        if failures.is_empty() {
            original
        } else {
            AruError::msg(format!(
                "{original}; preparation cleanup left paths for review: {}",
                failures.join("; ")
            ))
        }
    }
}

fn same_directory(created: &std::fs::Metadata, current: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        current.is_dir() && created.dev() == current.dev() && created.ino() == current.ino()
    }
    #[cfg(not(unix))]
    {
        let _ = created;
        current.is_dir() && !current.file_type().is_symlink()
    }
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
