use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::{Operation, apply_at, destination_exists, recover_if_needed_at};
use crate::error::{AruError, IoContext, Result};

pub fn apply_standalone(project: &Path, operations: Vec<Operation>, force: bool) -> Result<()> {
    if operations.is_empty() {
        return Ok(());
    }
    apply_standalone_prepared(project, move || {
        if !force {
            for operation in &operations {
                let destination = project.join(&operation.destination);
                if destination_exists(&destination) {
                    return Err(AruError::msg(format!(
                        "collision: unmanaged entry already exists at {}; inspect it or rerun with --force",
                        operation.destination.display()
                    )));
                }
            }
        }
        Ok((operations, ()))
    })
}

pub fn apply_standalone_prepared<T>(
    project: &Path,
    prepare: impl FnOnce() -> Result<(Vec<Operation>, T)>,
) -> Result<T> {
    let (_lock, journal_path) = acquire(project)?;
    recover_if_needed_at(project, &journal_path)?;
    if project.join(crate::manifest::MANIFEST_FILE).is_file() {
        return Err(AruError::msg(
            "aru.toml appeared during standalone installation; retry the command",
        ));
    }
    let (operations, output) = prepare()?;
    apply_at(project, operations, &journal_path)?;
    Ok(output)
}

fn acquire(project: &Path) -> Result<(File, PathBuf)> {
    let canonical = project.canonicalize().at(project)?;
    let digest = crate::digest::sha256_bytes(canonical.as_os_str().as_encoded_bytes());
    let control = std::env::temp_dir()
        .join("aru-standalone")
        .join(digest.strip_prefix("sha256:").unwrap_or(&digest));
    std::fs::create_dir_all(&control).at(&control)?;
    let lock_path = control.join("operation.lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .at(&lock_path)?;
    lock.lock_exclusive().map_err(|error| {
        AruError::msg(format!(
            "could not acquire standalone operation lock: {error}"
        ))
    })?;
    Ok((lock, control.join("transaction.toml")))
}
