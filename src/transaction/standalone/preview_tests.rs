use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn first_use_preview_blocks_lock_creation_without_writing() {
    for existing_control in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let control = root.path().canonicalize().unwrap().join("control");
        if existing_control {
            std::fs::create_dir(&control).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let preview = lock_without_pending_journal_at(&control).unwrap();
        assert!(preview._file.is_none());
        assert!(preview._bootstrap.is_some());
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
        assert_eq!(control.exists(), existing_control);
        assert!(!control.join("operation.lock").exists());
        assert!(!control.join("transaction.toml").exists());
        drop(preview);
        acquired_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        writer.join().unwrap();
        assert!(control.join("operation.lock").is_file());
        assert!(!control.join("transaction.toml").exists());
    }
}

#[test]
fn established_preview_releases_bootstrap_but_keeps_its_user_lock() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().canonicalize().unwrap();
    let control = root_path.join("control");
    let (file, _) = acquire_global_at(control.clone()).unwrap();
    drop(file);
    let preview = lock_without_pending_journal_at(&control).unwrap();
    assert!(preview._file.is_some());
    assert!(preview._bootstrap.is_none());
    let competing = OpenOptions::new()
        .read(true)
        .write(true)
        .open(control.join("operation.lock"))
        .unwrap();
    assert!(competing.try_lock_exclusive().is_err());

    let other_control = root_path.join("other-user");
    let (tx, rx) = mpsc::channel();
    let other = std::thread::spawn(move || {
        let (_lock, _) = acquire_global_at(other_control).unwrap();
        tx.send(()).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    other.join().unwrap();
    drop(preview);
    competing.try_lock_exclusive().unwrap();
}
