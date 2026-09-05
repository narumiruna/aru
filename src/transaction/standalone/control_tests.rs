use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn legacy_scope_lookup_skips_absent_roots_and_foreign_owners() {
    use std::os::unix::fs::MetadataExt;

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    assert!(
        legacy_control_directory_at(&project, &root.path().join("missing"))
            .unwrap()
            .is_none()
    );
    let control = legacy_control_directory_at(&project, root.path())
        .unwrap()
        .unwrap();
    std::fs::create_dir_all(&control).unwrap();
    std::fs::write(control.join("transaction.toml"), "untrusted pending state").unwrap();
    let uid = unsafe { libc::geteuid() };
    assert!(
        !inspect_legacy_scope(&control, |metadata| metadata.uid() == uid.wrapping_add(1)).unwrap()
    );
    assert!(owned_legacy_scope(&control).unwrap());
    assert_eq!(
        std::fs::read(control.join("transaction.toml")).unwrap(),
        b"untrusted pending state"
    );
    assert!(!control.join("operation.lock").exists());
}

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
fn established_fallback_survives_unused_home_symlinks() {
    use std::os::unix::fs::MetadataExt;

    for pending in [false, true] {
        for private in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let root = root.path().canonicalize().unwrap();
            let home = root.join("missing-home");
            let fallback = root.join("fallback");
            let uid = unsafe { libc::geteuid() };
            let selected = select_unix_control_directory(Some(&home), uid, &fallback).unwrap();
            assert_eq!(selected, fallback);
            let (lock, journal) = acquire_global_at(selected).unwrap();
            let inode = lock.metadata().unwrap().ino();
            let destination = root.join("demo");
            if pending {
                std::fs::write(&destination, b"old").unwrap();
                super::super::set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
                let crashed = apply_absolute_at(
                    vec![Operation::file(&destination, b"new".to_vec())],
                    &journal,
                    true,
                );
                super::super::set_failure_phase("ARU_TEST_CRASH_AFTER", None);
                assert!(crashed.is_err());
            }
            let retained = std::fs::read(&journal).ok();
            drop(lock);
            let mode = if private { 0o700 } else { 0o755 };
            std::fs::set_permissions(&fallback, std::fs::Permissions::from_mode(mode)).unwrap();

            std::fs::create_dir(&home).unwrap();
            let unused = root.join("unused-state");
            std::fs::create_dir(&unused).unwrap();
            #[cfg(target_os = "macos")]
            let alias = home.join("Library");
            #[cfg(not(target_os = "macos"))]
            let alias = home.join(".local");
            std::os::unix::fs::symlink(&unused, &alias).unwrap();

            let selected = select_unix_control_directory(Some(&home), uid, &fallback).unwrap();
            assert_eq!(selected, fallback);
            let preview = lock_without_pending_journal_at(&selected);
            assert_eq!(preview.is_ok(), private && !pending);
            if let Ok(preview) = preview {
                assert_eq!(preview._file.metadata().unwrap().ino(), inode);
            }
            assert_eq!(std::fs::read(&journal).ok(), retained);
            assert_eq!(fallback.metadata().unwrap().mode() & 0o777, mode);
            let (_lock, selected_journal) = acquire_global_at(selected).unwrap();
            assert_eq!(selected_journal, journal);
            assert_eq!(_lock.metadata().unwrap().ino(), inode);
            assert_eq!(recover_standalone_if_needed_at(&journal).unwrap(), pending);
            if pending {
                assert_eq!(std::fs::read(&destination).unwrap(), b"old");
            }
            assert!(!journal.exists());
            assert_eq!(std::fs::read_link(&alias).unwrap(), unused);
            assert_eq!(std::fs::read_dir(&unused).unwrap().count(), 0);
            assert_eq!(std::fs::read_dir(&home).unwrap().count(), 1);
        }
    }
}

#[test]
fn established_fallback_still_rejects_its_own_symlink_ancestor() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let real = root.join("real");
    let alias = root.join("alias");
    let fallback = real.join("fallback");
    let (lock, journal) = acquire_global_at(fallback).unwrap();
    drop(lock);
    std::fs::write(&journal, b"pending").unwrap();
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let home = root.join("home");
    std::fs::create_dir(&home).unwrap();
    let result = select_unix_control_directory(
        Some(&home),
        unsafe { libc::geteuid() },
        &alias.join("fallback"),
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(&journal).unwrap(), b"pending");
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
        true,
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
