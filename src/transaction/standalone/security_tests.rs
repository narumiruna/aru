use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn unsafe_journals_are_rejected_before_directory_permission_repair() {
    for entry in ["transaction.toml", "transaction.toml.tmp"] {
        for kind in ["symlink", "hardlink", "directory", "writable"] {
            let root = tempfile::tempdir().unwrap();
            let root = root.path().canonicalize().unwrap();
            let control = root.join("control");
            std::fs::create_dir(&control).unwrap();
            std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o755)).unwrap();
            let unrelated = root.join("preserve");
            std::fs::write(&unrelated, b"unrelated").unwrap();
            let path = control.join(entry);
            match kind {
                "symlink" => symlink(&unrelated, &path).unwrap(),
                "hardlink" => std::fs::hard_link(&unrelated, &path).unwrap(),
                "directory" => std::fs::create_dir(&path).unwrap(),
                "writable" => {
                    std::fs::write(&path, b"injected").unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666))
                        .unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                acquire_global_at(control.clone()).is_err(),
                "{entry}/{kind}"
            );
            assert!(lock_without_pending_journal_at(&control).is_err());
            assert_eq!(
                control.metadata().unwrap().permissions().mode() & 0o777,
                0o755
            );
            assert!(!control.join("operation.lock").exists());
            assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated");
            assert!(path.symlink_metadata().is_ok());
        }
    }
}

#[test]
fn writable_control_directory_is_not_automatically_trusted_or_repaired() {
    let root = tempfile::tempdir().unwrap();
    let control = root.path().canonicalize().unwrap().join("control");
    std::fs::create_dir(&control).unwrap();
    std::fs::set_permissions(&control, std::fs::Permissions::from_mode(0o777)).unwrap();
    std::fs::write(control.join("transaction.toml"), b"untrusted journal").unwrap();
    assert!(acquire_global_at(control.clone()).is_err());
    assert_eq!(
        control.metadata().unwrap().permissions().mode() & 0o777,
        0o777
    );
    assert_eq!(
        std::fs::read(control.join("transaction.toml")).unwrap(),
        b"untrusted journal"
    );
    assert!(!control.join("operation.lock").exists());
}

#[test]
fn standalone_relative_and_symlink_roots_are_persisted_canonically() {
    // Reach a disposable unmanaged root without changing process-wide CWD.
    let temporary = tempfile::tempdir().unwrap();
    let mut relative = PathBuf::new();
    for _ in std::env::current_dir().unwrap().ancestors().skip(1) {
        relative.push("..");
    }
    relative.push(temporary.path().strip_prefix("/").unwrap());
    let project = relative.as_path();
    assert!(!project.is_absolute());
    apply_standalone(
        project,
        vec![Operation::file("a", b"local".to_vec())],
        false,
    )
    .unwrap();
    apply_standalone_prepared(project, || {
        Ok((vec![Operation::file("b", b"prepared".to_vec())], ()))
    })
    .unwrap();
    drop(StandaloneDryRun::begin(project, false).unwrap());
    assert_eq!(std::fs::read(project.join("a")).unwrap(), b"local");

    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let original = root.join("original");
    let other = root.join("other");
    let alias = root.join("alias");
    std::fs::create_dir(&original).unwrap();
    std::fs::create_dir(&other).unwrap();
    symlink(&original, &alias).unwrap();
    crate::transaction::set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone(
        &alias,
        vec![Operation::file("demo", b"new".to_vec())],
        false,
    );
    crate::transaction::set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());
    std::fs::remove_file(&alias).unwrap();
    symlink(&other, &alias).unwrap();
    std::fs::write(other.join("demo"), b"unrelated").unwrap();
    let (_lock, journal) = acquire_global().unwrap();
    assert!(recover_standalone_if_needed_at(&journal).unwrap());
    assert!(!original.join("demo").exists());
    assert_eq!(std::fs::read(other.join("demo")).unwrap(), b"unrelated");
}
