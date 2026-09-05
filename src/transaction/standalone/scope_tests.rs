use super::*;

#[test]
fn scope_marker_fixture_has_stable_round_trip() {
    let fixture = include_str!("../../../tests/fixtures/contracts/standalone-scope.toml");
    let scope: Scope = toml::from_str(fixture).unwrap();
    assert_eq!(toml::to_string_pretty(&scope).unwrap(), fixture);
}

#[test]
fn home_outage_cannot_switch_a_pinned_scope_or_hide_its_journal() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("home");
    std::fs::create_dir(&home).unwrap();
    let anchor = root.join("anchor");
    let fallback = root.join("fallback");
    let uid = unsafe { libc::geteuid() };
    let (guard, control) = select_at(Some(&home), uid, None, false, &anchor, &fallback).unwrap();
    let (lock, journal) = acquire_global_at(control.clone()).unwrap();
    let destination = root.join("demo");
    std::fs::write(&destination, b"old").unwrap();
    crate::transaction::set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_absolute_at(
        vec![Operation::file(&destination, b"new".to_vec())],
        &journal,
        true,
    );
    crate::transaction::set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());
    let retained = std::fs::read(&journal).unwrap();
    let marker = std::fs::read(anchor.join("scope.toml")).unwrap();
    drop((lock, guard));

    let unavailable = root.join("unmounted-home");
    std::fs::rename(&home, &unavailable).unwrap();
    for preview in [false, true] {
        assert!(select_at(Some(&home), uid, None, preview, &anchor, &fallback).is_err());
    }
    assert!(!fallback.exists());
    assert!(!home.exists());
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir_all(&control).unwrap();
    assert!(select_at(Some(&home), uid, None, false, &anchor, &fallback).is_err());
    assert_eq!(std::fs::read(anchor.join("scope.toml")).unwrap(), marker);
    std::fs::remove_dir_all(&home).unwrap();
    std::fs::rename(&unavailable, &home).unwrap();
    assert_eq!(std::fs::read(&journal).unwrap(), retained);
    let (_guard, selected) = select_at(Some(&home), uid, None, false, &anchor, &fallback).unwrap();
    assert_eq!(selected, control);
    let (_lock, journal) = acquire_global_at(selected).unwrap();
    assert!(recover_standalone_if_needed_at(&journal).unwrap());
    assert_eq!(std::fs::read(destination).unwrap(), b"old");
}

#[test]
fn home_project_uses_external_scope_without_creating_project_metadata() {
    for preview in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let home = root.join("home");
        std::fs::create_dir(&home).unwrap();
        let anchor = root.join("anchor");
        let fallback = root.join("fallback");
        let uid = unsafe { libc::geteuid() };
        let (_guard, selected) =
            select_at(Some(&home), uid, Some(&home), preview, &anchor, &fallback).unwrap();
        assert_eq!(selected, fallback);
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
        assert!(anchor.join("scope.toml").is_file());
        drop(lock_without_pending_journal_at(&selected).unwrap());
        assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
    }
}

#[test]
fn existing_home_scope_is_not_silently_switched_for_a_different_project() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("home");
    std::fs::create_dir(&home).unwrap();
    let anchor = root.join("anchor");
    let fallback = root.join("fallback");
    let uid = unsafe { libc::geteuid() };
    let (guard, control) = select_at(Some(&home), uid, None, false, &anchor, &fallback).unwrap();
    drop(guard);
    let marker = std::fs::read(anchor.join("scope.toml")).unwrap();
    for preview in [false, true] {
        assert!(select_at(Some(&home), uid, Some(&home), preview, &anchor, &fallback).is_err());
    }
    assert!(!fallback.exists());
    assert!(!control.join("operation.lock").exists());
    assert_eq!(std::fs::read(anchor.join("scope.toml")).unwrap(), marker);
}

#[test]
fn established_fallback_anchor_ignores_unused_home_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("home");
    let unused = root.join("unused");
    std::fs::create_dir(&unused).unwrap();
    std::os::unix::fs::symlink(&unused, &home).unwrap();
    let anchor = root.join("anchor");
    let fallback = root.join("fallback");
    let (lock, _) = acquire_global_at(fallback.clone()).unwrap();
    drop(lock);
    let uid = unsafe { libc::geteuid() };
    let (guard, selected) = select_at(Some(&home), uid, None, true, &anchor, &fallback).unwrap();
    assert_eq!(selected, fallback);
    drop(guard);
    std::fs::remove_file(&home).unwrap();
    let (_guard, selected) = select_at(Some(&home), uid, None, false, &anchor, &fallback).unwrap();
    assert_eq!(selected, fallback);
    assert_eq!(std::fs::read_dir(&unused).unwrap().count(), 0);
}

#[test]
fn scope_anchor_is_held_until_the_operation_finishes() {
    use std::sync::mpsc;
    use std::time::Duration;

    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("home");
    std::fs::create_dir(&home).unwrap();
    let anchor = root.join("anchor");
    let fallback = root.join("fallback");
    let uid = unsafe { libc::geteuid() };
    let (guard, selected) = select_at(Some(&home), uid, None, true, &anchor, &fallback).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    let other = std::thread::spawn(move || {
        started_tx.send(()).unwrap();
        let (_guard, control) =
            select_at(Some(&home), uid, None, false, &anchor, &fallback).unwrap();
        tx.send(control).unwrap();
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    drop(guard);
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), selected);
    other.join().unwrap();
}

#[test]
fn unanchored_missing_home_fails_closed_without_creating_a_fallback() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let home = root.join("missing-home");
    let anchor = root.join("anchor");
    let fallback = root.join("fallback");
    for preview in [false, true, false] {
        assert!(
            select_at(
                Some(&home),
                unsafe { libc::geteuid() },
                None,
                preview,
                &anchor,
                &fallback
            )
            .is_err()
        );
    }
    assert!(!home.exists());
    assert!(!fallback.exists());
    assert!(!anchor.join("scope.toml").exists());
}
