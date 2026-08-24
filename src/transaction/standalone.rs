use std::fs::OpenOptions;
use std::path::Path;

use fs2::FileExt;

use super::{Operation, apply_at, destination_exists, recover_if_needed_at};
use crate::error::{AruError, IoContext, Result};

pub fn apply_standalone(project: &Path, operations: Vec<Operation>, force: bool) -> Result<()> {
    if operations.is_empty() {
        return Ok(());
    }
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
    if project.join(crate::manifest::MANIFEST_FILE).is_file() {
        return Err(AruError::msg(
            "aru.toml appeared during standalone skill installation; retry the command",
        ));
    }
    let journal_path = control.join("transaction.toml");
    recover_if_needed_at(project, &journal_path)?;
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
    apply_at(project, operations, &journal_path)
}
