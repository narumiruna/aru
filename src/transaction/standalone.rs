use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::{
    Operation, PathMode, apply_absolute_at, apply_standalone_at, destination_exists,
    recover_if_needed_at, recover_standalone_if_needed_at, validate_operations,
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
    recover_standalone_if_needed_at(&journal_path)?;
    validate_standalone_root(project, "global skill")?;
    validate_operations(PathMode::Absolute, &operations, 2)?;
    validate_no_managed_recovery(&operations)?;
    validate_collisions(&operations, force, Path::to_path_buf)?;
    apply_absolute_at(operations, &journal_path)
}

pub fn validate_standalone_dry_run(project: &Path, operations: &[Operation]) -> Result<()> {
    validate_no_pending_journal()?;
    validate_standalone_root(project, "standalone")?;
    validate_operations(PathMode::Project(project), operations, 2)
}

pub fn validate_standalone_global_dry_run(project: &Path, operations: &[Operation]) -> Result<()> {
    validate_no_pending_journal()?;
    validate_standalone_root(project, "global skill")?;
    validate_operations(PathMode::Absolute, operations, 2)?;
    validate_no_managed_recovery(operations)
}

pub(super) fn validate_no_pending_journal() -> Result<()> {
    let control = global_control_directory()?;
    let lock_path = control.join("operation.lock");
    let _lock = if lock_path.exists() {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .at(&lock_path)?;
        lock.lock_exclusive().map_err(|error| {
            AruError::msg(format!(
                "could not acquire standalone operation lock: {error}"
            ))
        })?;
        Some(lock)
    } else {
        None
    };
    if control.join("transaction.toml").exists() {
        return Err(AruError::msg(
            "a recoverable standalone transaction is pending; run a mutating aru command before --dry-run",
        ));
    }
    Ok(())
}

fn validate_no_managed_recovery(operations: &[Operation]) -> Result<()> {
    let mut roots = BTreeSet::new();
    for operation in operations {
        for ancestor in operation.destination.ancestors() {
            if ancestor.join(crate::manifest::MANIFEST_FILE).is_file()
                || ancestor.join(super::JOURNAL_FILE).is_file()
            {
                roots.insert(ancestor.to_path_buf());
            }
        }
    }
    for root in roots {
        let journal = root.join(super::JOURNAL_FILE);
        if journal.exists() {
            return Err(AruError::msg(format!(
                "a recoverable managed transaction at {} overlaps a global destination; run a mutating aru command in that project first",
                root.display()
            )));
        }
    }
    Ok(())
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
    recover_standalone_if_needed_at(&journal_path)?;
    validate_standalone_root(project, "standalone")?;
    let (operations, output) = prepare()?;
    validate_operations(PathMode::Project(project), &operations, 2)?;
    apply_standalone_at(project, operations, &journal_path)?;
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

pub(super) fn acquire_global() -> Result<(File, PathBuf)> {
    acquire_control(global_control_directory()?)
}

#[cfg(not(test))]
fn global_control_directory() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let root = dirs::data_local_dir().ok_or_else(|| {
        AruError::msg(
            "could not determine a stable user state directory for standalone transactions",
        )
    })?;
    #[cfg(target_os = "macos")]
    let root = stable_user_home()?.join("Library/Application Support");
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = stable_user_home()?.join(".local/state");
    Ok(root.join("aru/standalone"))
}

#[cfg(all(not(test), unix, not(target_os = "redox")))]
fn stable_user_home() -> Result<PathBuf> {
    use std::ffi::{CStr, OsString};
    use std::mem;
    use std::os::unix::ffi::OsStringExt;
    use std::ptr;

    let size = match unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) } {
        value if value < 0 => 512,
        value => (value as usize).max(512),
    };
    let mut buffer = Vec::<u8>::with_capacity(size);
    let mut passwd: libc::passwd = unsafe { mem::zeroed() };
    let mut result = ptr::null_mut();
    let status = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut passwd,
            buffer.as_mut_ptr().cast(),
            buffer.capacity(),
            &mut result,
        )
    };
    if status != 0 || result.is_null() || passwd.pw_dir.is_null() {
        return Err(AruError::msg(
            "could not determine a stable home directory for standalone transaction state",
        ));
    }
    let bytes = unsafe { CStr::from_ptr(passwd.pw_dir) }.to_bytes();
    if bytes.is_empty() {
        return Err(AruError::msg(
            "could not determine a stable home directory for standalone transaction state",
        ));
    }
    Ok(PathBuf::from(OsString::from_vec(bytes.to_vec())))
}

#[cfg(all(
    not(test),
    not(target_os = "windows"),
    not(all(unix, not(target_os = "redox")))
))]
fn stable_user_home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| {
        AruError::msg(
            "could not determine a stable home directory for standalone transaction state",
        )
    })
}

#[cfg(test)]
fn global_control_directory() -> Result<PathBuf> {
    Ok(std::env::temp_dir()
        .join("aru-standalone-tests")
        .join(std::process::id().to_string())
        .join(format!("{:?}", std::thread::current().id())))
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
