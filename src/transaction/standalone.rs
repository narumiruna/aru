use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use super::{
    Operation, PathMode, apply_absolute_at, apply_standalone_at, destination_exists,
    normalize_destination, recover_if_needed_at, recover_standalone_if_needed_at,
    validate_operations,
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
    validate_no_managed_overlap(&operations)?;
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
    validate_no_managed_overlap(operations)
}

pub(crate) fn validate_no_pending_journal() -> Result<()> {
    let control = global_control_directory()?;
    if control.exists() {
        validate_control_directory(&control, false)?;
    }
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

fn validate_no_managed_overlap(operations: &[Operation]) -> Result<()> {
    let mut roots = BTreeSet::new();
    for operation in operations {
        collect_managed_roots(&operation.destination, &mut roots);
        collect_managed_roots(&normalize_destination(&operation.destination)?, &mut roots);
    }
    for root in roots {
        if root.join(crate::manifest::MANIFEST_FILE).is_file() {
            return Err(AruError::msg(format!(
                "global destination is inside managed aru project {}; use the managed project instead",
                root.display()
            )));
        }
        if root.join(super::JOURNAL_FILE).exists() {
            return Err(AruError::msg(format!(
                "a recoverable managed transaction at {} overlaps a global destination; run a mutating aru command in that project first",
                root.display()
            )));
        }
    }
    Ok(())
}

fn collect_managed_roots(destination: &Path, roots: &mut BTreeSet<PathBuf>) {
    for ancestor in destination.ancestors() {
        if ancestor.join(crate::manifest::MANIFEST_FILE).is_file()
            || ancestor.join(super::JOURNAL_FILE).is_file()
        {
            roots.insert(ancestor.to_path_buf());
        }
    }
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

#[cfg(all(not(test), target_os = "windows"))]
fn global_control_directory() -> Result<PathBuf> {
    let root = dirs::data_local_dir().ok_or_else(|| {
        AruError::msg(
            "could not determine a stable user state directory for standalone transactions",
        )
    })?;
    Ok(root.join("aru/standalone"))
}

#[cfg(all(not(test), unix, not(target_os = "redox")))]
fn global_control_directory() -> Result<PathBuf> {
    let uid = unsafe { libc::geteuid() };
    Ok(unix_control_directory(stable_user_home()?.as_deref(), uid))
}

#[cfg(all(
    not(test),
    not(target_os = "windows"),
    not(all(unix, not(target_os = "redox")))
))]
fn global_control_directory() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        AruError::msg(
            "could not determine a stable home directory for standalone transaction state",
        )
    })?;
    Ok(home.join(".local/state/aru/standalone"))
}

#[cfg(all(unix, not(target_os = "redox")))]
fn unix_control_directory(home: Option<&Path>, uid: libc::uid_t) -> PathBuf {
    match home {
        #[cfg(target_os = "macos")]
        Some(home) => home.join("Library/Application Support/aru/standalone"),
        #[cfg(not(target_os = "macos"))]
        Some(home) => home.join(".local/state/aru/standalone"),
        None => PathBuf::from(format!("/var/tmp/aru-standalone-{uid}")),
    }
}

#[cfg(all(not(test), unix, not(target_os = "redox")))]
fn stable_user_home() -> Result<Option<PathBuf>> {
    use std::ffi::{CStr, OsString};
    use std::mem;
    use std::os::unix::ffi::OsStringExt;
    use std::ptr;

    let size = match unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) } {
        value if value < 0 => 512,
        value => (value as usize).max(512),
    };
    let mut buffer = vec![0_u8; size];
    let mut passwd: libc::passwd = unsafe { mem::zeroed() };
    let mut result = ptr::null_mut();
    let status = unsafe {
        libc::getpwuid_r(
            libc::geteuid(),
            &mut passwd,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        )
    };
    if status != 0 {
        return Err(AruError::msg(format!(
            "could not query the user account for standalone transaction state: OS error {status}"
        )));
    }
    if result.is_null() || passwd.pw_dir.is_null() {
        return Ok(None);
    }
    let bytes = unsafe { CStr::from_ptr(passwd.pw_dir) }.to_bytes();
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(OsString::from_vec(bytes.to_vec()))))
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
    validate_control_directory(&control, true)?;
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

fn validate_control_directory(control: &Path, repair_permissions: bool) -> Result<()> {
    #[cfg(not(unix))]
    let _ = repair_permissions;
    let metadata = std::fs::symlink_metadata(control).at(control)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AruError::msg(format!(
            "standalone transaction control path must be an owned directory: {}",
            control.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let uid = unsafe { libc::geteuid() };
        if metadata.uid() != uid {
            return Err(AruError::msg(format!(
                "standalone transaction control directory {} is not owned by the current user",
                control.display()
            )));
        }
        if repair_permissions {
            std::fs::set_permissions(control, std::fs::Permissions::from_mode(0o700))
                .at(control)?;
        } else if metadata.permissions().mode() & 0o077 != 0 {
            return Err(AruError::msg(format!(
                "standalone transaction control directory {} has unsafe permissions; run a mutating aru command to repair it",
                control.display()
            )));
        }
    }
    Ok(())
}

#[cfg(all(test, unix, not(target_os = "redox")))]
mod tests {
    use super::*;

    #[test]
    fn unix_control_directory_falls_back_to_durable_uid_path() {
        assert_eq!(
            unix_control_directory(None, 42),
            PathBuf::from("/var/tmp/aru-standalone-42")
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unix_control_directory_uses_account_home_when_available() {
        assert_eq!(
            unix_control_directory(Some(Path::new("/home/demo")), 42),
            PathBuf::from("/home/demo/.local/state/aru/standalone")
        );
    }

    #[test]
    fn control_directory_rejects_a_symlink() {
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        let alias = root.path().join("alias");
        std::fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();

        assert!(acquire_control(alias).is_err());
    }
}
