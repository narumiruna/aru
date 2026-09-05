use std::path::Path;

use crate::error::{AruError, Result};

/// Serializes lock-file discovery/creation without creating filesystem state.
/// An initial dry-run retains this guard until its preview is complete; once a
/// per-user file lock is held, the bootstrap guard can be released.
#[cfg(unix)]
pub(super) struct BootstrapLock {
    _file: std::fs::File,
}

#[cfg(windows)]
pub(super) struct BootstrapLock(windows_sys::Win32::Foundation::HANDLE);

#[cfg(not(any(unix, windows)))]
pub(super) struct BootstrapLock;

impl BootstrapLock {
    pub(super) fn acquire(control: &Path) -> Result<Self> {
        #[cfg(unix)]
        {
            use crate::error::IoContext;
            use fs2::FileExt;

            let _ = control;
            // This existing system directory is independent of HOME/TMPDIR and
            // account availability. flock does not write directory contents.
            let path = Path::new("/var/tmp");
            let file = std::fs::File::open(path).at(path)?;
            file.lock_exclusive().map_err(|error| {
                AruError::msg(format!(
                    "could not acquire standalone bootstrap lock: {error}"
                ))
            })?;
            Ok(Self { _file: file })
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0};
            use windows_sys::Win32::System::Threading::{
                CreateMutexW, INFINITE, WaitForSingleObject,
            };

            // Global namespace coordinates sessions; the stable user control
            // identity avoids sharing the default security descriptor across users.
            let identity = crate::digest::sha256_bytes(control.as_os_str().as_encoded_bytes());
            let name = format!("Global\\aru-standalone-bootstrap-{identity}")
                .encode_utf16()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if handle.is_null() {
                return Err(AruError::msg(format!(
                    "could not create standalone bootstrap mutex: {}",
                    std::io::Error::last_os_error()
                )));
            }
            let result = unsafe { WaitForSingleObject(handle, INFINITE) };
            if !matches!(result, WAIT_OBJECT_0 | WAIT_ABANDONED) {
                let error = std::io::Error::last_os_error();
                unsafe { CloseHandle(handle) };
                return Err(AruError::msg(format!(
                    "could not acquire standalone bootstrap mutex: {error}"
                )));
            }
            Ok(Self(handle))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = control;
            Err(AruError::msg(
                "standalone bootstrap locking is unsupported on this platform",
            ))
        }
    }
}

#[cfg(windows)]
impl Drop for BootstrapLock {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.0);
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
