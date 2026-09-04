use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::{
    Operation, apply_absolute_at, apply_at, destination_exists, recover_absolute_if_needed_at,
    recover_if_needed_at,
};
use crate::error::{AruError, IoContext, Result};

pub fn apply_standalone(project: &Path, operations: Vec<Operation>, force: bool) -> Result<()> {
    if operations.is_empty() {
        return Ok(());
    }
    apply_standalone_prepared(project, move || {
        validate_collisions(&operations, force, |destination| project.join(destination))?;
        Ok((operations, ()))
    })
}

pub fn apply_standalone_global(
    project: &Path,
    operations: Vec<Operation>,
    force: bool,
) -> Result<()> {
    if operations.is_empty() {
        return Ok(());
    }
    let (_lock, journal_path) = acquire_global()?;
    recover_absolute_if_needed_at(&journal_path)?;
    if project.join(crate::manifest::MANIFEST_FILE).is_file() {
        return Err(AruError::msg(
            "aru.toml appeared during global skill installation; retry the command",
        ));
    }
    validate_collisions(&operations, force, Path::to_path_buf)?;
    apply_absolute_at(operations, &journal_path)
}

fn validate_collisions(
    operations: &[Operation],
    force: bool,
    resolve: impl Fn(&Path) -> PathBuf,
) -> Result<()> {
    if force {
        return Ok(());
    }
    for operation in operations {
        let destination = resolve(&operation.destination);
        if destination_exists(&destination) {
            return Err(AruError::msg(format!(
                "collision: unmanaged entry already exists at {}; inspect it or rerun with --force",
                operation.destination.display()
            )));
        }
    }
    Ok(())
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
    acquire_control(
        std::env::temp_dir()
            .join("aru-standalone")
            .join(digest.strip_prefix("sha256:").unwrap_or(&digest)),
    )
}

fn acquire_global() -> Result<(File, PathBuf)> {
    acquire_control(global_control_directory()?)
}

fn global_control_directory() -> Result<PathBuf> {
    let root = std::env::temp_dir().join("aru-standalone");
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        std::fs::create_dir_all(&root).at(&root)?;
        let probe = tempfile::tempfile_in(&root)
            .map_err(|error| AruError::msg(format!("could not identify current user: {error}")))?;
        let user = probe
            .metadata()
            .map_err(|error| AruError::msg(format!("could not identify current user: {error}")))?
            .uid();
        Ok(root.join(format!("global-{user}")))
    }
    #[cfg(not(unix))]
    {
        Ok(root.join("global"))
    }
}

fn acquire_control(control: PathBuf) -> Result<(File, PathBuf)> {
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
