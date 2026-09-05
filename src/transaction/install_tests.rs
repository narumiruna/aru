use super::*;
use crate::transaction::{Operation, apply_standalone, apply_standalone_global};

struct HookGuard;
impl Drop for HookGuard {
    fn drop(&mut self) {
        HOOK.with_borrow_mut(|hook| *hook = None);
    }
}

#[test]
fn non_force_installs_preserve_concurrent_content_during_staging_and_commit() {
    for global in [false, true] {
        for event in [Event::Staged, Event::Installing] {
            for directory in [false, true] {
                let project = tempfile::tempdir().unwrap();
                let outside = tempfile::tempdir().unwrap();
                let root = if global {
                    outside.path()
                } else {
                    project.path()
                };
                let source = tempfile::tempdir().unwrap();
                std::fs::write(source.path().join("SKILL.md"), b"same-content").unwrap();
                let path = |name| {
                    if global {
                        root.join(name)
                    } else {
                        std::path::PathBuf::from(name)
                    }
                };
                let operations = vec![
                    Operation::file(path("a"), b"first-install".to_vec()),
                    if directory {
                        Operation::directory(path("b"), source.path())
                    } else {
                        Operation::file(path("b"), b"same-content".to_vec())
                    },
                ];
                let _guard = HookGuard;
                HOOK.with_borrow_mut(|hook| {
                    *hook = Some(Box::new(move |phase, destination| {
                        if phase == event && destination.file_name().unwrap() == "b" {
                            if directory {
                                std::fs::create_dir(destination).unwrap();
                                std::fs::write(destination.join("SKILL.md"), b"same-content")
                                    .unwrap();
                            } else {
                                std::fs::write(destination, b"same-content").unwrap();
                            }
                        }
                    }))
                });
                let result = if global {
                    apply_standalone_global(project.path(), operations, false)
                } else {
                    apply_standalone(project.path(), operations, false)
                };
                assert!(result.is_err());
                assert!(!root.join("a").exists(), "earlier installs must roll back");
                let content = if directory {
                    root.join("b/SKILL.md")
                } else {
                    root.join("b")
                };
                assert_eq!(std::fs::read(content).unwrap(), b"same-content");
                assert_eq!(
                    std::fs::read_dir(root).unwrap().count(),
                    1,
                    "no stages or backups may remain"
                );
                let (_lock, journal) = crate::transaction::standalone::acquire_global().unwrap();
                assert!(!journal.exists());
            }
        }
    }
}

#[test]
fn preparation_cleanup_preserves_concurrent_content_in_new_parents() {
    for global in [false, true] {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root = if global {
            outside.path()
        } else {
            project.path()
        };
        let _guard = HookGuard;
        HOOK.with_borrow_mut(|hook| {
            *hook = Some(Box::new(|phase, destination| {
                if phase == Event::Staged {
                    std::fs::write(destination.parent().unwrap().join("manual"), b"preserve")
                        .unwrap();
                    // Force a late, non-force collision during preparation.
                    std::fs::write(destination, b"concurrent").unwrap();
                }
            }));
        });
        let path = if global {
            root.join("new/skills/demo")
        } else {
            "new/skills/demo".into()
        };
        let operations = vec![Operation::file(path, b"planned".to_vec())];
        let result = if global {
            apply_standalone_global(project.path(), operations, false)
        } else {
            apply_standalone(project.path(), operations, false)
        };
        let error = result.unwrap_err().to_string();
        assert!(error.contains("collision"));
        assert!(error.contains("cleanup left paths for review"));
        assert_eq!(
            std::fs::read(root.join("new/skills/manual")).unwrap(),
            b"preserve"
        );
        assert_eq!(
            std::fs::read(root.join("new/skills/demo")).unwrap(),
            b"concurrent"
        );
        assert_eq!(
            std::fs::read_dir(root.join("new/skills")).unwrap().count(),
            2
        );
        let (_lock, journal) = crate::transaction::standalone::acquire_global().unwrap();
        assert!(!journal.exists());
    }
}

#[test]
fn interrupted_create_only_transaction_recovers_without_removing_unapplied_content() {
    let project = tempfile::tempdir().unwrap();
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    crate::transaction::set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone_global(
        project.path(),
        vec![
            Operation::file(&a, b"new".to_vec()),
            Operation::file(&b, b"new".to_vec()),
        ],
        false,
    );
    crate::transaction::set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());
    std::fs::write(&b, b"manual").unwrap();
    let (lock, journal) = crate::transaction::standalone::acquire_global().unwrap();
    assert!(crate::transaction::recover_standalone_if_needed_at(&journal).unwrap());
    drop(lock);
    assert!(!a.exists());
    assert_eq!(std::fs::read(b).unwrap(), b"manual");
    assert!(!journal.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
))]
#[test]
fn exclusive_rename_installs_files_and_directories_at_absent_destinations() {
    for directory in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join("stage");
        let destination = root.path().join("destination");
        if directory {
            std::fs::create_dir(&stage).unwrap();
            std::fs::write(stage.join("SKILL.md"), b"new").unwrap();
        } else {
            std::fs::write(&stage, b"new").unwrap();
        }
        rename_no_replace(&stage, &destination).unwrap();
        assert!(!stage.exists());
        let content = if directory {
            destination.join("SKILL.md")
        } else {
            destination
        };
        assert_eq!(std::fs::read(content).unwrap(), b"new");
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "macos",
    windows
))]
#[test]
fn exclusive_rename_reports_missing_source() {
    let root = tempfile::tempdir().unwrap();
    let stage = root.path().join("missing");
    let destination = root.path().join("destination");
    let error = rename_exclusive(&stage, &destination).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!destination.exists());
}

#[test]
fn exclusive_rename_never_replaces_files_or_empty_directories() {
    for directory in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let stage = root.path().join("stage");
        let destination = root.path().join("destination");
        if directory {
            std::fs::create_dir(&stage).unwrap();
            std::fs::create_dir(&destination).unwrap();
        } else {
            std::fs::write(&stage, b"new").unwrap();
            std::fs::write(&destination, b"manual").unwrap();
        }
        assert!(rename_no_replace(&stage, &destination).is_err());
        if directory {
            assert!(stage.is_dir());
            assert!(destination.is_dir());
        } else {
            assert_eq!(std::fs::read(&stage).unwrap(), b"new");
            assert_eq!(std::fs::read(&destination).unwrap(), b"manual");
        }
    }
}

#[cfg(unix)]
#[test]
fn exclusive_rename_preserves_dangling_destination_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let stage = root.path().join("stage");
    let destination = root.path().join("destination");
    std::fs::write(&stage, b"new").unwrap();
    std::os::unix::fs::symlink("missing", &destination).unwrap();
    assert!(rename_no_replace(&stage, &destination).is_err());
    assert_eq!(
        std::fs::read_link(&destination).unwrap(),
        Path::new("missing")
    );
    assert_eq!(std::fs::read(&stage).unwrap(), b"new");
}
