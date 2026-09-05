use super::*;

#[test]
fn preparation_failure_removes_new_parents_but_preserves_existing_directories() {
    for absolute in [false, true] {
        for existing in [false, true] {
            let root = tempfile::tempdir().unwrap();
            let root = root.path().canonicalize().unwrap();
            let targets = root.join("targets");
            let source = root.join("source");
            std::fs::create_dir(&source).unwrap();
            std::fs::write(source.join("SKILL.md"), b"source").unwrap();
            if existing {
                std::fs::create_dir_all(targets.join("a")).unwrap();
                std::fs::write(targets.join("a/manual"), b"preserve").unwrap();
            }
            let before = path_digest(&targets).unwrap();
            let path = |name| {
                let relative = PathBuf::from(format!("targets/{name}/skills/demo"));
                if absolute {
                    root.join(relative)
                } else {
                    relative
                }
            };
            let operations = vec![
                Operation::file(path("a"), b"first stage".to_vec()),
                Operation::skill_directory(path("b"), &source, "sha256:wrong"),
            ];
            let journal = root.join("control/transaction.toml");
            let result = if absolute {
                apply_absolute_at(operations, &journal, false)
            } else {
                apply_standalone_at(&root, operations, &journal, false)
            };
            let error = result.unwrap_err().to_string();
            assert!(error.contains("post-copy skill digest"), "{error}");
            assert!(!error.contains("cleanup left paths"), "{error}");
            assert_eq!(path_digest(&targets).unwrap(), before);
            assert!(!journal.exists());
        }
    }
}

#[test]
fn initial_journal_failure_also_cleans_preparation_parents() {
    let root = tempfile::tempdir().unwrap();
    let targets = root.path().join("targets");
    let journal = root.path().join("control/transaction.toml");
    let temporary = journal.with_extension("toml.tmp");
    std::fs::create_dir_all(&temporary).unwrap();
    let result = apply_absolute_at(
        vec![Operation::file(
            targets.join("nested/demo"),
            b"stage".to_vec(),
        )],
        &journal,
        false,
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("unsafe transaction state file")
    );
    assert!(!targets.exists());
    assert!(!journal.exists());
    assert!(temporary.is_dir());
}

#[cfg(unix)]
#[test]
fn later_unwritable_target_cleans_earlier_preparation_parents() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        return; // Root bypasses directory write permissions.
    }
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("a");
    let second = root.path().join("b");
    std::fs::create_dir(&second).unwrap();
    std::fs::set_permissions(&second, std::fs::Permissions::from_mode(0o555)).unwrap();
    let journal = root.path().join("control/transaction.toml");
    let result = apply_absolute_at(
        vec![
            Operation::file(first.join("nested/skills/demo"), b"first".to_vec()),
            Operation::file(second.join("skills/demo"), b"second".to_vec()),
        ],
        &journal,
        false,
    );
    std::fs::set_permissions(&second, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
    assert!(!first.exists());
    assert_eq!(std::fs::read_dir(&second).unwrap().count(), 0);
    assert!(!journal.exists());
}

#[test]
fn partial_parent_creation_is_tracked_before_an_error() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first");
    let mut staging = Staging::default();
    staging.create_parents(&first).unwrap();
    std::fs::write(first.join("not-a-directory"), b"manual").unwrap();
    let error = staging
        .create_parents(&first.join("not-a-directory/child"))
        .unwrap_err();
    let error = staging.cleanup(error).to_string();
    assert!(error.contains("cleanup left paths for review"));
    assert_eq!(
        std::fs::read(first.join("not-a-directory")).unwrap(),
        b"manual"
    );
}

#[cfg(unix)]
#[test]
fn cleanup_preserves_replaced_parent_identity() {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("parent");
    let saved = root.path().join("saved");
    let mut staging = Staging::default();
    staging.create_parents(&parent).unwrap();
    std::fs::rename(&parent, &saved).unwrap();
    std::fs::create_dir(&parent).unwrap();
    let error = staging
        .cleanup(AruError::msg("preparation failed"))
        .to_string();
    assert!(error.contains("was replaced; preserved for review"));
    assert!(parent.is_dir());
    assert!(saved.is_dir());
}
