use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn first_use_preview_creates_only_private_lock_metadata_and_blocks_its_writer() {
    for existing_control in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let control = root_path.join("control");
        let project = root_path.join("project");
        std::fs::create_dir(&project).unwrap();
        if existing_control {
            std::fs::create_dir(&control).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let preview = lock_without_pending_journal_at(&control).unwrap();
        assert!(control.join("operation.lock").is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let directory = control.metadata().unwrap();
            let file = preview._file.metadata().unwrap();
            assert_eq!(directory.mode() & 0o777, 0o700);
            assert_eq!(file.mode() & 0o777, 0o600);
            assert_eq!(directory.uid(), unsafe { libc::geteuid() });
            assert_eq!(file.uid(), directory.uid());
            assert_eq!(file.nlink(), 1);
        }
        let writer_control = control.clone();
        let (started_tx, started_rx) = mpsc::channel();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let writer = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let (_lock, _) = acquire_global_at(writer_control).unwrap();
            acquired_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            acquired_rx.recv_timeout(Duration::from_millis(100)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));

        // Even a waiting writer must not block another user's first preview or
        // mutation. Separate scopes model separate OS-user state directories.
        let other_control = root_path.join("other-user");
        let (tx, rx) = mpsc::channel();
        let other = std::thread::spawn(move || {
            drop(lock_without_pending_journal_at(&other_control).unwrap());
            let (_lock, _) = acquire_global_at(other_control).unwrap();
            tx.send(()).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        other.join().unwrap();
        assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&control).unwrap().count(), 1);
        assert!(!control.join("transaction.toml").exists());
        drop(preview);
        acquired_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        writer.join().unwrap();
        assert!(control.join("operation.lock").is_file());
        assert!(!control.join("transaction.toml").exists());
    }
}

#[test]
fn established_preview_keeps_the_existing_lock_identity() {
    let root = tempfile::tempdir().unwrap();
    let control = root.path().canonicalize().unwrap().join("control");
    let (file, _) = acquire_global_at(control.clone()).unwrap();
    drop(file);
    let competing = OpenOptions::new()
        .read(true)
        .write(true)
        .open(control.join("operation.lock"))
        .unwrap();
    let preview = lock_without_pending_journal_at(&control).unwrap();
    assert!(competing.try_lock_exclusive().is_err());
    drop(preview);
    competing.try_lock_exclusive().unwrap();
}

#[test]
fn preview_preserves_pending_journal_and_does_not_repair_unsafe_permissions() {
    let root = tempfile::tempdir().unwrap();
    let control = root.path().canonicalize().unwrap().join("control");
    let (file, journal) = acquire_global_at(control.clone()).unwrap();
    drop(file);
    std::fs::write(&journal, b"pending recovery").unwrap();
    assert!(lock_without_pending_journal_at(&control).is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), b"pending recovery");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(lock_without_pending_journal_at(&control).is_err());
        assert_eq!(
            control.metadata().unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}

#[cfg(unix)]
#[test]
fn preview_and_mutation_reject_unsafe_lock_entries_without_touching_content() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    for kind in [
        "symlink",
        "dangling",
        "hardlink",
        "directory",
        "fifo",
        "writable",
    ] {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let control = root_path.join("control");
        std::fs::create_dir(&control).unwrap();
        std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
        let unrelated = root_path.join("unrelated");
        std::fs::write(&unrelated, b"preserve me").unwrap();
        let lock = control.join("operation.lock");
        match kind {
            "symlink" => symlink(&unrelated, &lock).unwrap(),
            "dangling" => symlink(root_path.join("absent"), &lock).unwrap(),
            "hardlink" => std::fs::hard_link(&unrelated, &lock).unwrap(),
            "directory" => std::fs::create_dir(&lock).unwrap(),
            "fifo" => {
                use std::os::unix::ffi::OsStrExt;
                let path = std::ffi::CString::new(lock.as_os_str().as_bytes()).unwrap();
                assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
            }
            "writable" => {
                std::fs::write(&lock, b"untrusted lock").unwrap();
                std::fs::set_permissions(&lock, std::fs::Permissions::from_mode(0o666)).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(lock_without_pending_journal_at(&control).is_err(), "{kind}");
        assert!(acquire_global_at(control.clone()).is_err(), "{kind}");
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"preserve me");
        assert!(!root_path.join("absent").exists());
        assert!(!control.join("transaction.toml").exists());
        assert!(lock.symlink_metadata().is_ok());
        if kind == "writable" {
            assert_eq!(std::fs::read(&lock).unwrap(), b"untrusted lock");
        }
    }
}
