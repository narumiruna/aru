use super::*;

#[test]
fn managed_work_recovers_legacy_standalone_journals_before_project_writes() {
    for drift in [false, true] {
        let project = tempfile::tempdir().unwrap();
        let legacy = standalone::legacy_control_directory(project.path())
            .unwrap()
            .unwrap();
        let journal = legacy.join("transaction.toml");
        std::fs::create_dir_all(&legacy).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let legacy_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(legacy.join("operation.lock"))
            .unwrap();
        std::fs::write(project.path().join("demo"), b"old").unwrap();
        set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
        let crashed = apply_at(
            project.path(),
            vec![Operation::file("demo", b"new".to_vec())],
            &journal,
        );
        set_failure_phase("ARU_TEST_CRASH_AFTER", None);
        assert!(crashed.is_err());
        if drift {
            std::fs::write(project.path().join("demo"), b"manual").unwrap();
        }
        let retained = std::fs::read(&journal).unwrap();
        assert!(crate::app::begin(project.path(), true).is_err());
        assert!(StandaloneDryRun::begin(project.path(), false).is_err());
        assert_eq!(std::fs::read(&journal).unwrap(), retained);
        assert!(!project.path().join(".aru").exists());

        let lock = ProjectLock::acquire(project.path());
        if drift {
            assert!(lock.is_err());
            assert_eq!(
                std::fs::read(project.path().join("demo")).unwrap(),
                b"manual"
            );
            assert_eq!(std::fs::read(&journal).unwrap(), retained);
            assert!(!project.path().join(".aru").exists());
        } else {
            let lock = lock.unwrap();
            assert_eq!(std::fs::read(project.path().join("demo")).unwrap(), b"old");
            assert!(!journal.exists());
            assert!(legacy_lock.try_lock_exclusive().is_err());
            let _inherited_legacy = lock._legacy_file.as_ref().unwrap().try_clone().unwrap();
            drop(lock);
            legacy_lock.try_lock_exclusive().unwrap();
        }
        drop(legacy_lock);
        std::fs::remove_dir_all(legacy).unwrap();
    }
}

#[test]
fn managed_preview_retains_shared_and_legacy_locks_until_completion() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("aru.toml"), b"project intent").unwrap();
    let before = path_digest(project.path()).unwrap();
    let legacy = standalone::legacy_control_directory(project.path())
        .unwrap()
        .unwrap();
    std::fs::create_dir_all(&legacy).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&legacy, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    File::create(legacy.join("operation.lock")).unwrap();
    let (lock, journal) = standalone::acquire_global().unwrap();
    drop(lock);
    let competing = [
        journal.with_file_name("operation.lock"),
        legacy.join("operation.lock"),
    ]
    .map(|path| {
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap()
    });

    // Both managed --dry-run and --check enter this shared begin path before
    // loading manifest, lockfile, ownership state or projections.
    let preview = crate::app::begin(project.path(), true).unwrap();
    for lock in &competing {
        assert!(lock.try_lock_exclusive().is_err());
    }
    assert_eq!(path_digest(project.path()).unwrap(), before);
    assert!(!journal.exists());
    assert!(!project.path().join(".aru").exists());
    drop(preview);
    for lock in &competing {
        lock.try_lock_exclusive().unwrap();
        FileExt::unlock(lock).unwrap();
    }

    // An error after acquiring the guard must also release both locks without
    // repairing the pending managed journal.
    std::fs::create_dir(project.path().join(".aru")).unwrap();
    let pending = project.path().join(JOURNAL_FILE);
    std::fs::write(&pending, b"pending recovery").unwrap();
    assert!(crate::app::begin(project.path(), true).is_err());
    assert_eq!(std::fs::read(&pending).unwrap(), b"pending recovery");
    for lock in &competing {
        lock.try_lock_exclusive().unwrap();
        FileExt::unlock(lock).unwrap();
    }
    drop(competing);
    std::fs::remove_dir_all(legacy).unwrap();
}

#[test]
fn case_ambiguous_destinations_are_rejected_before_any_staging() {
    for (left, right) in [
        ("Root/skills/demo", "root/skills/demo"),
        ("Root/skills/demo", "root/skills/demo/child"),
        ("Root/skills/Demo", "Root/skills/demo"),
        ("CAFÉ/skills/demo", "cafe\u{301}/skills/demo"),
        ("Straße/skills/demo", "STRASSE/skills/demo"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let journal = root.path().join("control/transaction.toml");
        let operations = || {
            vec![
                Operation::file(root.path().join(left), b"one".to_vec()),
                Operation::file(root.path().join(right), b"two".to_vec()),
            ]
        };
        let preview = StandaloneDryRun::begin(project.path(), true).unwrap();
        assert!(preview.validate(&operations()).is_err());
        drop(preview);
        assert!(apply_absolute_at(operations(), &journal, true).is_err());
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[cfg(unix)]
#[test]
fn global_recovery_keeps_resolved_paths_after_ancestor_symlink_changes() {
    for replace in [false, true] {
        let project = tempfile::tempdir().unwrap();
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original");
        let other = root.path().join("other");
        let alias = root.path().join("alias");
        std::fs::create_dir(&original).unwrap();
        std::fs::create_dir(&other).unwrap();
        std::os::unix::fs::symlink(&original, &alias).unwrap();
        if replace {
            std::fs::write(original.join("demo"), b"old").unwrap();
        }
        std::fs::write(other.join("demo"), b"unrelated").unwrap();

        set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
        let crashed = apply_standalone_global(
            project.path(),
            vec![Operation::file(alias.join("demo"), b"new".to_vec())],
            replace,
        );
        set_failure_phase("ARU_TEST_CRASH_AFTER", None);
        assert!(crashed.is_err());
        assert_eq!(std::fs::read(original.join("demo")).unwrap(), b"new");
        let (_lock, journal_path) = standalone::acquire_global().unwrap();
        let journal = read_journal(&journal_path).unwrap().unwrap();
        let entry = &journal.entries[0];
        for stored in [
            Some(&entry.destination),
            entry.stage.as_ref(),
            entry.backup.as_ref(),
        ]
        .into_iter()
        .flatten()
        {
            let path = decode_journal_path(journal.version, PathMode::Absolute, stored).unwrap();
            assert_eq!(path.parent().unwrap(), original.canonicalize().unwrap());
        }
        std::fs::remove_file(&alias).unwrap();
        std::os::unix::fs::symlink(&other, &alias).unwrap();

        assert!(recover_standalone_if_needed_at(&journal_path).unwrap());
        if replace {
            assert_eq!(std::fs::read(original.join("demo")).unwrap(), b"old");
        } else {
            assert!(!original.join("demo").exists());
        }
        assert_eq!(std::fs::read(other.join("demo")).unwrap(), b"unrelated");
        assert_eq!(
            std::fs::read_dir(&original).unwrap().count(),
            usize::from(replace)
        );
        assert!(!journal_path.exists());
    }
}

#[test]
fn standalone_rechecks_all_project_ancestors_before_prepare_or_preview() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("nested/project");
    std::fs::create_dir_all(&project).unwrap();
    let output = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(crate::manifest::MANIFEST_FILE), "").unwrap();

    let local = apply_standalone_prepared(&project, || -> Result<(Vec<Operation>, ())> {
        panic!("must reject the new managed ancestor before preparing writes");
    });
    assert!(local.unwrap_err().to_string().contains("aru.toml appeared"));
    assert!(StandaloneDryRun::begin(&project, false).is_err());
    let operations = vec![Operation::file(output.path().join("demo"), b"new".to_vec())];
    let preview = StandaloneDryRun::begin(&project, true).unwrap();
    preview.validate(&operations).unwrap();
    drop(preview);
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 0);
    apply_standalone_global(&project, operations, false).unwrap();
    assert_eq!(std::fs::read(output.path().join("demo")).unwrap(), b"new");
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
}

#[test]
fn standalone_preview_holds_lock_through_collision_inspection_and_validation() {
    let project = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let destination = output.path().join("demo");
    let (lock, journal) = standalone::acquire_global().unwrap();
    let lock_path = journal.with_file_name("operation.lock");
    drop(lock);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let writer_destination = destination.clone();
    let writer_lock_path = lock_path.clone();
    let writer = std::thread::spawn(move || {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .open(writer_lock_path)
            .unwrap();
        lock.lock_exclusive().unwrap();
        ready_tx.send(()).unwrap();
        std::fs::write(writer_destination, b"installed").unwrap();
    });
    ready_rx.recv().unwrap();

    let preview = StandaloneDryRun::begin(project.path(), true).unwrap();
    assert!(destination_exists(&destination));
    preview
        .validate(&[Operation::file(&destination, Vec::new())])
        .unwrap();
    let competing = OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    assert!(competing.try_lock_exclusive().is_err());
    drop(preview);
    competing.try_lock_exclusive().unwrap();
    writer.join().unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), b"installed");
    assert!(!journal.exists());
}
