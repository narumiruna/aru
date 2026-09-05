use std::path::Path;

use crate::error::{AruError, Result};

// A check followed by std::fs::rename can still overwrite concurrent content.
// Never fall back to that sequence when the filesystem lacks no-replace rename.
pub(super) fn rename_no_replace(stage: &Path, destination: &Path) -> Result<()> {
    rename_exclusive(stage, destination).map_err(|error| {
        AruError::msg(format!(
            "could not install without replacing unmanaged destination {}: {error}",
            destination.display()
        ))
    })
}

#[cfg(any(target_os = "linux", target_os = "android", target_os = "macos"))]
fn rename_exclusive(stage: &Path, destination: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let stage = CString::new(stage.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // Both pointers refer to live, NUL-terminated path buffers for this call.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            stage.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE as _,
        )
    };
    #[cfg(target_os = "macos")]
    let result =
        unsafe { libc::renamex_np(stage.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn rename_exclusive(stage: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    let wide = |path: &Path| -> std::io::Result<Vec<u16>> {
        let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if value.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path contains NUL",
            ));
        }
        value.push(0);
        Ok(value)
    };
    let stage = wide(stage)?;
    let destination = wide(destination)?;
    // No MOVEFILE_REPLACE_EXISTING: a destination created by a concurrent writer
    // must cause failure. The stage is a sibling, so no cross-volume copy occurs.
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            stage.as_ptr(),
            destination.as_ptr(),
            0,
        )
    };
    if result != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
)))]
fn rename_exclusive(_stage: &Path, _destination: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "atomic no-replace installation is unsupported on this platform",
    ))
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Event {
    Staged,
    Installing,
}

#[cfg(test)]
type Hook = Box<dyn FnMut(Event, &Path)>;

#[cfg(test)]
thread_local! { static HOOK: std::cell::RefCell<Option<Hook>> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
pub(super) fn run_hook(event: Event, destination: &Path) {
    HOOK.with_borrow_mut(|hook| {
        if let Some(hook) = hook {
            hook(event, destination);
        }
    });
}

#[cfg(test)]
#[path = "install_tests.rs"]
mod tests;
