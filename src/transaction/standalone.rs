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

mod bootstrap;
use bootstrap::BootstrapLock;

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

/// Keeps collision inspection and plan validation inside the same lock scope.
pub struct StandaloneDryRun<'a> {
    _lock: PreviewLock,
    project: &'a Path,
    global: bool,
}

impl<'a> StandaloneDryRun<'a> {
    pub fn begin(project: &'a Path, global: bool) -> Result<Self> {
        let lock = lock_without_pending_journal(project)?;
        validate_standalone_root(project, "standalone")?;
        Ok(Self {
            _lock: lock,
            project,
            global,
        })
    }

    pub fn validate(&self, operations: &[Operation]) -> Result<()> {
        validate_standalone_root(self.project, "standalone")?;
        if self.global {
            validate_operations(PathMode::Absolute, operations, 2)?;
            validate_no_managed_overlap(operations)
        } else {
            validate_operations(PathMode::Project(self.project), operations, 2)
        }
    }
}

pub(crate) fn validate_no_pending_journal(project: &Path) -> Result<()> {
    lock_without_pending_journal(project).map(|_| ())
}

struct PreviewLock {
    _file: Option<File>,
    _bootstrap: Option<BootstrapLock>,
    _legacy_file: Option<File>,
}

fn lock_without_pending_journal(project: &Path) -> Result<PreviewLock> {
    let mut lock = lock_without_pending_journal_at(&global_control_directory()?)?;
    // The shared guard is already held. Do not acquire bootstrap recursively.
    lock._legacy_file = lock_existing_without_pending_journal(&legacy_control_directory(project)?)?;
    Ok(lock)
}

fn lock_without_pending_journal_at(control: &Path) -> Result<PreviewLock> {
    let bootstrap = BootstrapLock::acquire(control)?;
    let lock = lock_existing_without_pending_journal(control)?;
    let bootstrap = if lock.is_some() {
        drop(bootstrap);
        None
    } else {
        Some(bootstrap)
    };
    Ok(PreviewLock {
        _file: lock,
        _bootstrap: bootstrap,
        _legacy_file: None,
    })
}

fn lock_existing_without_pending_journal(control: &Path) -> Result<Option<File>> {
    validate_control_ancestors(control)?;
    if control.symlink_metadata().is_ok() {
        validate_control_directory(control, false)?;
    }
    let lock_path = control.join("operation.lock");
    let lock = if lock_path.exists() {
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
    Ok(lock)
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
    if project
        .ancestors()
        .any(|ancestor| ancestor.join(crate::manifest::MANIFEST_FILE).is_file())
    {
        return Err(AruError::msg(format!(
            "aru.toml appeared during {operation} installation; retry the command"
        )));
    }
    Ok(())
}

pub(super) fn acquire_for_project(project: &Path) -> Result<(File, Option<File>, PathBuf)> {
    let (global_lock, journal_path) = acquire_global()?;
    let legacy_control = legacy_control_directory(project)?;
    let legacy_lock = match legacy_control.symlink_metadata() {
        Ok(_) => {
            let (lock, journal) = acquire_control(legacy_control)?;
            recover_if_needed_at(project, &journal)?;
            Some(lock)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(AruError::msg(format!(
                "could not inspect legacy standalone recovery scope {}: {error}",
                legacy_control.display()
            )));
        }
    };
    Ok((global_lock, legacy_lock, journal_path))
}

pub(super) fn legacy_control_directory(project: &Path) -> Result<PathBuf> {
    let canonical = project.canonicalize().at(project)?;
    let digest = crate::digest::sha256_bytes(canonical.as_os_str().as_encoded_bytes());
    let temporary = std::env::temp_dir();
    Ok(temporary
        .canonicalize()
        .at(&temporary)?
        .join("aru-standalone")
        .join(digest.strip_prefix("sha256:").unwrap_or(&digest)))
}

pub(super) fn acquire_global() -> Result<(File, PathBuf)> {
    acquire_global_at(global_control_directory()?)
}

fn acquire_global_at(control: PathBuf) -> Result<(File, PathBuf)> {
    let _bootstrap = BootstrapLock::acquire(&control)?;
    acquire_control(control)
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
    unix_control_directory(stable_user_home()?.as_deref(), uid)
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
fn unix_control_directory(home: Option<&Path>, uid: libc::uid_t) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    let temporary = "/private/var/tmp";
    #[cfg(not(target_os = "macos"))]
    let temporary = "/var/tmp";
    let fallback = PathBuf::from(format!("{temporary}/aru-standalone-{uid}"));
    select_unix_control_directory(home, uid, &fallback)
}

#[cfg(all(unix, not(target_os = "redox")))]
fn select_unix_control_directory(
    home: Option<&Path>,
    uid: libc::uid_t,
    fallback: &Path,
) -> Result<PathBuf> {
    let Some(home) = home.filter(|home| home.is_absolute()) else {
        return Ok(fallback.to_path_buf());
    };
    #[cfg(target_os = "macos")]
    let control = home.join("Library/Application Support/aru/standalone");
    #[cfg(not(target_os = "macos"))]
    let control = home.join(".local/state/aru/standalone");

    validate_control_ancestors(&control)?;
    // Never bypass an established lock or abandon its recovery journal when
    // permissions change. A selected fallback also stays sticky once created.
    if control.join("operation.lock").symlink_metadata().is_ok()
        || control.join("transaction.toml").symlink_metadata().is_ok()
    {
        return Ok(control);
    }
    if established_fallback_scope(fallback, uid) || !home.is_dir() {
        return Ok(fallback.to_path_buf());
    }
    let existing = control.ancestors().find(|path| path.exists());
    if existing.is_some_and(writable_directory) {
        Ok(control)
    } else {
        Ok(fallback.to_path_buf())
    }
}

#[cfg(all(unix, not(target_os = "redox")))]
fn writable_directory(path: &Path) -> bool {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    if !path.is_dir() {
        return false;
    }
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe {
        libc::faccessat(
            libc::AT_FDCWD,
            path.as_ptr(),
            libc::W_OK | libc::X_OK,
            libc::AT_EACCESS,
        ) == 0
    }
}

#[cfg(all(not(test), unix, not(target_os = "redox")))]
fn stable_user_home() -> Result<Option<PathBuf>> {
    let size = match unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) } {
        value if value < 0 => 512,
        value => value as usize,
    };
    lookup_user_home(size, |passwd, buffer, result| unsafe {
        libc::getpwuid_r(
            libc::geteuid(),
            passwd,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            result,
        )
    })
}

#[cfg(all(unix, not(target_os = "redox")))]
const MAX_PASSWD_BUFFER: usize = 1024 * 1024;

#[cfg(all(unix, not(target_os = "redox")))]
fn lookup_user_home(
    initial_size: usize,
    mut lookup: impl FnMut(&mut libc::passwd, &mut [u8], &mut *mut libc::passwd) -> libc::c_int,
) -> Result<Option<PathBuf>> {
    use std::ffi::{CStr, OsString};
    use std::os::unix::ffi::OsStringExt;

    let mut buffer = vec![0_u8; initial_size.clamp(512, MAX_PASSWD_BUFFER)];
    loop {
        let mut passwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result = std::ptr::null_mut();
        let status = lookup(&mut passwd, &mut buffer, &mut result);
        if status == libc::ERANGE && buffer.len() < MAX_PASSWD_BUFFER {
            buffer.resize((buffer.len() * 2).min(MAX_PASSWD_BUFFER), 0);
            continue;
        }
        if status != 0 {
            return Err(AruError::msg(format!(
                "could not query the user account for standalone transaction state: OS error {status} (buffer {} bytes, limit {MAX_PASSWD_BUFFER})",
                buffer.len()
            )));
        }
        if result.is_null() || passwd.pw_dir.is_null() {
            return Ok(None);
        }
        let bytes = unsafe { CStr::from_ptr(passwd.pw_dir) }.to_bytes();
        if bytes.is_empty() {
            return Ok(None);
        }
        return Ok(Some(PathBuf::from(OsString::from_vec(bytes.to_vec()))));
    }
}

#[cfg(test)]
fn global_control_directory() -> Result<PathBuf> {
    let temporary = std::env::temp_dir();
    Ok(temporary
        .canonicalize()
        .at(&temporary)?
        .join("aru-standalone-tests")
        .join(std::process::id().to_string())
        .join(format!("{:?}", std::thread::current().id())))
}

fn acquire_control(control: PathBuf) -> Result<(File, PathBuf)> {
    validate_control_ancestors(&control)?;
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

// Resolving a symlink anew on each invocation cannot retain a crashed journal's
// identity. Reject such control scopes rather than silently creating a second
// lock after retargeting. Existing state is left untouched for manual recovery.
fn validate_control_ancestors(control: &Path) -> Result<()> {
    for ancestor in control.ancestors() {
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AruError::msg(format!(
                    "standalone control path has a mutable symlink ancestor: {}; restore the original state path before retrying",
                    ancestor.display()
                )));
            }
            Ok(_) => (),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) => {}
            Err(error) => {
                return Err(AruError::msg(format!(
                    "could not inspect standalone control ancestor {}: {error}",
                    ancestor.display()
                )));
            }
        }
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "redox")))]
fn established_fallback_scope(path: &Path, uid: libc::uid_t) -> bool {
    use std::os::unix::fs::MetadataExt;

    path.symlink_metadata().is_ok_and(|metadata| {
        metadata.is_dir()
            && metadata.uid() == uid
            && (metadata.mode() & 0o077 == 0
                // Preserve owned recovery state across permission changes;
                // preview rejects unsafe permissions and mutation repairs them.
                || path.join("operation.lock").symlink_metadata().is_ok()
                || path.join("transaction.toml").symlink_metadata().is_ok())
    })
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

#[cfg(test)]
#[path = "standalone/preview_tests.rs"]
mod preview_tests;

#[cfg(all(test, unix, not(target_os = "redox")))]
#[path = "standalone/control_tests.rs"]
mod control_tests;

#[cfg(all(test, unix, not(target_os = "redox")))]
mod tests {
    use super::*;

    #[test]
    fn passwd_lookup_retries_erange_until_the_record_fits() {
        let mut sizes = Vec::new();
        let home = lookup_user_home(512, |passwd, buffer, result| {
            sizes.push(buffer.len());
            if buffer.len() < 4096 {
                return libc::ERANGE;
            }
            let home = b"/home/demo\0";
            buffer[..home.len()].copy_from_slice(home);
            passwd.pw_dir = buffer.as_mut_ptr().cast();
            *result = passwd;
            0
        })
        .unwrap();
        assert_eq!(sizes, [512, 1024, 2048, 4096]);
        assert_eq!(home, Some(PathBuf::from("/home/demo")));
    }

    #[test]
    fn passwd_lookup_growth_is_bounded() {
        let mut sizes = Vec::new();
        let result = lookup_user_home(512, |_, buffer, _| {
            sizes.push(buffer.len());
            libc::ERANGE
        });
        assert!(result.is_err());
        assert_eq!(sizes.len(), 12);
        assert_eq!(sizes.last(), Some(&MAX_PASSWD_BUFFER));
        let result = lookup_user_home(usize::MAX, |_, buffer, _| {
            assert_eq!(buffer.len(), MAX_PASSWD_BUFFER);
            libc::ERANGE
        });
        assert!(result.is_err());
    }

    #[test]
    fn passwd_lookup_preserves_missing_records_and_other_errors() {
        assert_eq!(lookup_user_home(512, |_, _, _| 0).unwrap(), None);
        let mut calls = 0;
        let result = lookup_user_home(512, |_, _, _| {
            calls += 1;
            libc::EIO
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn unix_control_directory_falls_back_to_durable_uid_path() {
        assert_eq!(
            unix_control_directory(None, 42)
                .unwrap()
                .file_name()
                .unwrap(),
            "aru-standalone-42"
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn unix_control_directory_uses_account_home_when_available() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            unix_control_directory(Some(home.path()), unsafe { libc::geteuid() }).unwrap(),
            home.path().join(".local/state/aru/standalone")
        );
    }

    #[test]
    fn unix_control_directory_rejects_unusable_account_homes() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let missing = root.join("nonexistent");
        let file = root.join("not-a-directory");
        std::fs::write(&file, "").unwrap();
        for home in [&missing, &file, Path::new("relative")] {
            assert_eq!(
                unix_control_directory(Some(home), 42).unwrap(),
                unix_control_directory(None, 42).unwrap()
            );
        }
        assert!(!missing.exists());
    }

    #[test]
    fn unix_control_directory_falls_back_from_unwritable_state_ancestor() {
        use std::os::unix::fs::PermissionsExt;

        // Root can write through mode 0555; this regression needs an unprivileged UID.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let home = tempfile::tempdir().unwrap();
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let selected =
            unix_control_directory(Some(&home.path().canonicalize().unwrap()), 42).unwrap();
        std::fs::set_permissions(home.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(selected, unix_control_directory(None, 42).unwrap());
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
    }

    #[test]
    fn unix_control_directory_does_not_abandon_existing_recovery_scope() {
        use std::os::unix::fs::PermissionsExt;

        let temporary = tempfile::tempdir().unwrap();
        let home = temporary.path().canonicalize().unwrap();
        let control = unix_control_directory(Some(&home), 42).unwrap();
        std::fs::create_dir_all(&control).unwrap();
        std::fs::write(control.join("transaction.toml"), "pending").unwrap();
        std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o555)).unwrap();
        let selected = unix_control_directory(Some(&home), 42).unwrap();
        std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(selected, control);
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
