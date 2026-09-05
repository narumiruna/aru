use super::*;

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
    let global = apply_standalone_global(
        &project,
        vec![Operation::file(output.path().join("demo"), b"new".to_vec())],
        false,
    );
    assert!(
        global
            .unwrap_err()
            .to_string()
            .contains("aru.toml appeared")
    );
    for global in [false, true] {
        assert!(StandaloneDryRun::begin(&project, global).is_err());
    }
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 0);
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
