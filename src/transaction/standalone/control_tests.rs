use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn unsafe_fallback_entries_do_not_override_a_usable_home() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("home");
    std::fs::create_dir(&home).unwrap();
    let fallback = root.join("fallback");
    let uid = unsafe { libc::geteuid() };
    let normal = select_unix_control_directory(Some(&home), uid, &fallback).unwrap();
    assert!(normal.starts_with(&home));

    std::fs::write(&fallback, "unrelated").unwrap();
    assert_eq!(
        select_unix_control_directory(Some(&home), uid, &fallback).unwrap(),
        normal
    );
    assert_eq!(std::fs::read(&fallback).unwrap(), b"unrelated");
    std::fs::remove_file(&fallback).unwrap();
    std::os::unix::fs::symlink(&home, &fallback).unwrap();
    assert_eq!(
        select_unix_control_directory(Some(&home), uid, &fallback).unwrap(),
        normal
    );
    assert!(fallback.is_symlink());
    std::fs::remove_file(&fallback).unwrap();
    std::fs::create_dir(&fallback).unwrap();
    std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        select_unix_control_directory(Some(&home), uid, &fallback).unwrap(),
        normal
    );
    std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o700)).unwrap();
    // Inject the selecting UID instead of requiring root/chown in routine tests.
    assert_eq!(
        select_unix_control_directory(Some(&home), uid.wrapping_add(1), &fallback).unwrap(),
        normal
    );
    assert_eq!(
        select_unix_control_directory(Some(&home), uid, &fallback).unwrap(),
        fallback
    );
    std::fs::write(fallback.join("transaction.toml"), "pending").unwrap();
    std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        select_unix_control_directory(Some(&home), uid, &fallback).unwrap(),
        fallback
    );
    assert_eq!(
        select_unix_control_directory(Some(&home), uid.wrapping_add(1), &fallback).unwrap(),
        normal
    );
    assert!(lock_without_pending_journal_at(&fallback).is_err());
    assert_eq!(
        std::fs::read(fallback.join("transaction.toml")).unwrap(),
        b"pending"
    );
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
}

#[test]
fn retargeted_control_ancestor_cannot_select_a_fresh_lock_or_hide_recovery() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("home");
    std::fs::create_dir(&home).unwrap();
    let fallback = root.join("fallback");
    let uid = unsafe { libc::geteuid() };
    let control = select_unix_control_directory(Some(&home), uid, &fallback).unwrap();
    let (lock, journal) = acquire_global_at(control.clone()).unwrap();
    let destination = root.join("demo");
    std::fs::write(&destination, b"old").unwrap();
    super::super::set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_absolute_at(
        vec![Operation::file(&destination, b"new".to_vec())],
        &journal,
    );
    super::super::set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());
    let retained = std::fs::read(&journal).unwrap();
    drop(lock);

    let state_ancestor = control.parent().unwrap();
    let original = root.join("original-state");
    let replacement = root.join("replacement-state");
    std::fs::rename(state_ancestor, &original).unwrap();
    std::fs::create_dir(&replacement).unwrap();
    std::os::unix::fs::symlink(&replacement, state_ancestor).unwrap();
    assert!(select_unix_control_directory(Some(&home), uid, &fallback).is_err());
    assert!(acquire_global_at(control.clone()).is_err());
    assert!(lock_without_pending_journal_at(&control).is_err());
    assert_eq!(
        std::fs::read(original.join("standalone/transaction.toml")).unwrap(),
        retained
    );
    assert_eq!(std::fs::read_dir(&replacement).unwrap().count(), 0);
    assert!(!fallback.exists());
    assert_eq!(std::fs::read(&destination).unwrap(), b"new");

    std::fs::remove_file(state_ancestor).unwrap();
    std::fs::rename(&original, state_ancestor).unwrap();
    let (_lock, journal) = acquire_global_at(control).unwrap();
    assert!(recover_standalone_if_needed_at(&journal).unwrap());
    assert_eq!(std::fs::read(destination).unwrap(), b"old");
    assert!(!journal.exists());
}
