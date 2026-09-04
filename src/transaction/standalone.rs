use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::{
    Operation, apply_absolute_at, destination_exists, recover_absolute_if_needed_at,
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
    let (_global_lock, _legacy_lock, journal_path) = acquire_for_project(project)?;
    recover_absolute_if_needed_at(&journal_path)?;
    validate_standalone_root(project, "global skill")?;
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
    let (_global_lock, _legacy_lock, journal_path) = acquire_for_project(project)?;
    recover_absolute_if_needed_at(&journal_path)?;
    validate_standalone_root(project, "standalone")?;
    let (mut operations, output) = prepare()?;
    for operation in &mut operations {
        if operation.destination.is_absolute() {
            return Err(AruError::msg(
                "standalone transaction destination must be project-relative",
            ));
        }
        operation.destination = project.join(&operation.destination);
    }
    apply_absolute_at(operations, &journal_path)?;
    Ok(output)
}

fn validate_standalone_root(project: &Path, operation: &str) -> Result<()> {
    if project.join(crate::manifest::MANIFEST_FILE).is_file() {
        return Err(AruError::msg(format!(
            "aru.toml appeared during {operation} installation; retry the command"
        )));
    }
    Ok(())
}

fn acquire_for_project(project: &Path) -> Result<(File, File, PathBuf)> {
    let (global_lock, journal_path) = acquire_global()?;
    let (legacy_lock, legacy_journal_path) = acquire_legacy(project)?;
    recover_if_needed_at(project, &legacy_journal_path)?;
    Ok((global_lock, legacy_lock, journal_path))
}

fn acquire_legacy(project: &Path) -> Result<(File, PathBuf)> {
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

#[cfg(not(test))]
fn global_control_directory() -> Result<PathBuf> {
    let root = match std::env::var_os("XDG_STATE_HOME") {
        Some(value) if !value.is_empty() => {
            let path = PathBuf::from(value);
            if !path.is_absolute() {
                return Err(AruError::msg(
                    "XDG_STATE_HOME must be an absolute path for standalone transaction state",
                ));
            }
            path
        }
        _ => dirs::state_dir().or_else(dirs::data_local_dir).ok_or_else(|| {
            AruError::msg(
                "could not determine a durable user state directory for standalone transactions",
            )
        })?,
    };
    if !root.is_absolute() {
        return Err(AruError::msg(
            "durable user state directory must be an absolute path",
        ));
    }
    Ok(root.join("aru/standalone"))
}

#[cfg(test)]
fn global_control_directory() -> Result<PathBuf> {
    Ok(std::env::temp_dir()
        .join("aru-standalone-tests")
        .join(std::process::id().to_string()))
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
