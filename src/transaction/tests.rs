use super::*;

#[path = "review_tests.rs"]
mod review;

#[test]
fn failed_post_copy_verification_leaves_no_stage_or_destination() {
    let project = tempfile::tempdir().unwrap();
    let source = project.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(
        source.join("SKILL.md"),
        "---\nname: source\ndescription: Source\n---\n# Source\n",
    )
    .unwrap();
    let result = apply(
        project.path(),
        vec![Operation::skill_directory(
            "skills/source",
            &source,
            "sha256:not-the-content",
        )],
    );
    assert!(result.is_err());
    assert!(!project.path().join("skills/source").exists());
    assert!(!project.path().join(JOURNAL_FILE).exists());
    assert!(
        std::fs::read_dir(project.path().join("skills"))
            .unwrap()
            .all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".aru-stage-"))
    );
}

#[test]
fn failed_instruction_transaction_restores_outputs_without_touching_sources() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join(".aru")).unwrap();
    std::fs::write(project.path().join("AGENTS.md"), "# User source\n").unwrap();
    std::fs::write(project.path().join("CLAUDE.md"), "old projection\n").unwrap();
    std::fs::write(project.path().join("aru.lock"), "old lock\n").unwrap();
    std::fs::write(project.path().join(".aru/state.toml"), "old state\n").unwrap();
    set_failure_phase("ARU_TEST_FAIL_AFTER", Some(2));
    let result = apply(
        project.path(),
        vec![
            Operation::file("CLAUDE.md", b"new projection\n".to_vec()),
            Operation::file("aru.lock", b"new lock\n".to_vec()),
            Operation::file(".aru/state.toml", b"new state\n".to_vec()),
        ],
    );
    set_failure_phase("ARU_TEST_FAIL_AFTER", None);
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(project.path().join("AGENTS.md")).unwrap(),
        b"# User source\n"
    );
    assert_eq!(
        std::fs::read(project.path().join("CLAUDE.md")).unwrap(),
        b"old projection\n"
    );
    assert_eq!(
        std::fs::read(project.path().join("aru.lock")).unwrap(),
        b"old lock\n"
    );
    assert_eq!(
        std::fs::read(project.path().join(".aru/state.toml")).unwrap(),
        b"old state\n"
    );
}

#[test]
fn failed_phase_rolls_back_all_destinations() {
    for phase in 1..=3 {
        let project = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(project.path().join(name), format!("old-{name}")).unwrap();
        }
        set_failure_phase("ARU_TEST_FAIL_AFTER", Some(phase));
        let result = apply(
            project.path(),
            ["a", "b", "c"]
                .into_iter()
                .map(|name| Operation::file(name, format!("new-{name}").into_bytes()))
                .collect(),
        );
        set_failure_phase("ARU_TEST_FAIL_AFTER", None);
        assert!(result.is_err());
        for name in ["a", "b", "c"] {
            assert_eq!(
                std::fs::read(project.path().join(name)).unwrap(),
                format!("old-{name}").as_bytes()
            );
        }
        assert!(!project.path().join(JOURNAL_FILE).exists());
    }
}

#[cfg(unix)]
#[test]
fn mixed_file_directory_and_symlink_transaction_rolls_back() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("config"), "old-config").unwrap();
    let old_skill = project.path().join("skill");
    std::fs::create_dir(&old_skill).unwrap();
    std::fs::write(old_skill.join("old"), "old-skill").unwrap();
    std::os::unix::fs::symlink("old-target", project.path().join("link")).unwrap();
    let source = project.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(
        source.join("SKILL.md"),
        "---\nname: source\ndescription: Source\n---\n# New\n",
    )
    .unwrap();
    let digest = crate::skill::canonical_skill_digest(&source).unwrap();
    set_failure_phase("ARU_TEST_FAIL_AFTER", Some(3));
    let result = apply(
        project.path(),
        vec![
            Operation::file("config", b"new-config".to_vec()),
            Operation::skill_directory("skill", &source, digest),
            Operation::symlink("link", "new-target"),
        ],
    );
    set_failure_phase("ARU_TEST_FAIL_AFTER", None);
    assert!(result.is_err());
    assert_eq!(
        std::fs::read(project.path().join("config")).unwrap(),
        b"old-config"
    );
    assert_eq!(
        std::fs::read(project.path().join("skill/old")).unwrap(),
        b"old-skill"
    );
    assert_eq!(
        std::fs::read_link(project.path().join("link")).unwrap(),
        PathBuf::from("old-target")
    );
}

#[test]
fn every_crash_phase_is_recovered_on_next_invocation() {
    for phase in 1..=3 {
        let project = tempfile::tempdir().unwrap();
        for name in ["a", "b", "c"] {
            std::fs::write(project.path().join(name), format!("old-{name}")).unwrap();
        }
        set_failure_phase("ARU_TEST_CRASH_AFTER", Some(phase));
        let result = apply(
            project.path(),
            ["a", "b", "c"]
                .into_iter()
                .map(|name| Operation::file(name, format!("new-{name}").into_bytes()))
                .collect(),
        );
        set_failure_phase("ARU_TEST_CRASH_AFTER", None);
        assert!(result.is_err());
        assert!(recover_if_needed(project.path()).unwrap());
        for name in ["a", "b", "c"] {
            assert_eq!(
                std::fs::read(project.path().join(name)).unwrap(),
                format!("old-{name}").as_bytes()
            );
        }
    }
}

#[test]
fn standalone_transaction_recovers_without_project_state() {
    let project = tempfile::tempdir().unwrap();
    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone(
        project.path(),
        vec![Operation::file("a", b"new".to_vec())],
        true,
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    apply_standalone(
        project.path(),
        vec![Operation::file("a", b"new".to_vec())],
        false,
    )
    .unwrap();
    assert_eq!(std::fs::read(project.path().join("a")).unwrap(), b"new");
    assert!(!project.path().join(".aru").exists());
    assert!(!project.path().join(JOURNAL_FILE).exists());
}

#[test]
fn global_transaction_uses_one_recovery_scope_for_different_destination_sets() {
    let project = tempfile::tempdir().unwrap();
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let first = first_root.path().join("skills/first");
    let second = second_root.path().join("skills/second");

    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone_global(
        project.path(),
        vec![
            Operation::file(&first, b"first".to_vec()),
            Operation::file(&second, b"second".to_vec()),
        ],
        true,
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    apply_standalone_global(
        project.path(),
        vec![Operation::file(&first, b"recovered".to_vec())],
        false,
    )
    .unwrap();
    assert_eq!(std::fs::read(&first).unwrap(), b"recovered");
    assert!(!second.exists());
}

#[test]
fn global_transaction_recovers_an_overlapping_project_scoped_install() {
    let project = tempfile::tempdir().unwrap();
    let other_project = tempfile::tempdir().unwrap();
    let destination = project.path().join(".kiro/skills/demo");

    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone(
        project.path(),
        vec![Operation::file(".kiro/skills/demo", b"project".to_vec())],
        true,
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    apply_standalone_global(
        other_project.path(),
        vec![Operation::file(&destination, b"global".to_vec())],
        false,
    )
    .unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), b"global");
}

#[cfg(unix)]
#[test]
fn absolute_transaction_supports_non_utf8_paths_losslessly() {
    use std::os::unix::ffi::OsStringExt;

    let root = tempfile::tempdir().unwrap();
    let invalid = std::ffi::OsString::from_vec(vec![b'n', b'o', b'n', b'-', 0xff]);
    let destination = root.path().join(invalid).join("skills/demo");
    let journal = root.path().join("control/transaction.toml");

    apply_absolute_at(
        vec![Operation::file(&destination, b"demo".to_vec())],
        &journal,
    )
    .unwrap();

    assert_eq!(std::fs::read(destination).unwrap(), b"demo");
    assert!(!journal.exists());
}

#[cfg(unix)]
#[test]
fn local_standalone_recovers_from_a_non_utf8_project_root() {
    use std::os::unix::ffi::OsStringExt;

    let root = tempfile::tempdir().unwrap();
    let invalid = std::ffi::OsString::from_vec(vec![b'p', b'r', b'o', b'j', b'-', 0xff]);
    let project = root.path().join(invalid);
    std::fs::create_dir(&project).unwrap();

    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone(
        &project,
        vec![Operation::file("first", b"first".to_vec())],
        true,
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    apply_standalone(
        &project,
        vec![Operation::file("second", b"second".to_vec())],
        false,
    )
    .unwrap();
    assert!(!project.join("first").exists());
    assert_eq!(std::fs::read(project.join("second")).unwrap(), b"second");
}

#[cfg(unix)]
#[test]
fn aliased_absolute_destinations_are_rejected_before_staging() {
    let root = tempfile::tempdir().unwrap();
    let real = root.path().join("real");
    let alias = root.path().join("alias");
    std::fs::create_dir_all(real.join("skills")).unwrap();
    std::os::unix::fs::symlink(&real, &alias).unwrap();
    let real_destination = real.join("skills/demo");
    let alias_destination = alias.join("skills/demo");
    let journal = root.path().join("control/transaction.toml");

    let result = apply_absolute_at(
        vec![
            Operation::file(&real_destination, b"real".to_vec()),
            Operation::file(&alias_destination, b"alias".to_vec()),
        ],
        &journal,
    );

    assert!(result.is_err());
    assert!(!real_destination.exists());
    assert!(!journal.exists());
}

#[test]
fn large_destination_plan_validates_without_pairwise_comparison() {
    let root = tempfile::tempdir().unwrap();
    let parent = root.path().join("skills");
    std::fs::create_dir(&parent).unwrap();
    let operations = (0..10_000)
        .map(|index| Operation::file(parent.join(format!("skill-{index:05}")), Vec::new()))
        .collect::<Vec<_>>();

    validate_operations(PathMode::Absolute, &operations, 2).unwrap();
}

#[test]
fn nested_absolute_destinations_are_rejected_before_staging() {
    let root = tempfile::tempdir().unwrap();
    let ancestor = root.path().join("skills/demo");
    let descendant = ancestor.join("skills/demo");
    let journal = root.path().join("control/transaction.toml");

    let result = apply_absolute_at(
        vec![
            Operation::file(&ancestor, b"ancestor".to_vec()),
            Operation::file(&descendant, b"descendant".to_vec()),
        ],
        &journal,
    );

    assert!(result.is_err());
    assert!(!ancestor.exists());
    assert!(!journal.exists());
}

#[test]
fn global_dry_run_rejects_pending_recovery_without_mutating_it() {
    let project = tempfile::tempdir().unwrap();
    let destination_root = tempfile::tempdir().unwrap();
    let destination = destination_root.path().join("skills/demo");

    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone_global(
        project.path(),
        vec![Operation::file(&destination, b"first".to_vec())],
        true,
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    let result = StandaloneDryRun::begin(project.path(), true)
        .and_then(|dry_run| dry_run.validate(&[Operation::file(&destination, b"second".to_vec())]));
    assert!(result.is_err());
    assert!(destination.exists());

    apply_standalone_global(
        project.path(),
        vec![Operation::file(&destination, b"second".to_vec())],
        false,
    )
    .unwrap();
    assert_eq!(std::fs::read(destination).unwrap(), b"second");
}

#[test]
fn managed_dry_run_rejects_pending_standalone_recovery_without_mutating_it() {
    let standalone = tempfile::tempdir().unwrap();
    let managed = tempfile::tempdir().unwrap();
    let destination_root = tempfile::tempdir().unwrap();
    let destination = destination_root.path().join("skills/demo");

    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone_global(
        standalone.path(),
        vec![Operation::file(&destination, b"global".to_vec())],
        true,
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    let result = crate::app::begin(managed.path(), true);
    assert!(result.is_err());
    assert!(destination.exists());

    let _lock = ProjectLock::acquire(managed.path()).unwrap();
    assert!(!destination.exists());
}

#[test]
fn managed_lock_recovers_pending_standalone_transaction() {
    let standalone = tempfile::tempdir().unwrap();
    let managed = tempfile::tempdir().unwrap();
    let destination = managed.path().join(".kiro/skills/demo");

    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply_standalone_global(
        standalone.path(),
        vec![Operation::file(&destination, b"global".to_vec())],
        true,
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    let _lock = ProjectLock::acquire(managed.path()).unwrap();
    assert!(!destination.exists());
}

#[test]
fn global_transaction_rejects_pending_managed_recovery() {
    let managed = tempfile::tempdir().unwrap();
    let standalone = tempfile::tempdir().unwrap();
    let destination = managed.path().join(".kiro/skills/demo");

    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    let crashed = apply(
        managed.path(),
        vec![Operation::file(".kiro/skills/demo", b"managed".to_vec())],
    );
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    assert!(crashed.is_err());

    let result = apply_standalone_global(
        standalone.path(),
        vec![Operation::file(&destination, b"global".to_vec())],
        true,
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(&destination).unwrap(), b"managed");

    assert!(recover_if_needed(managed.path()).unwrap());
    assert!(!destination.exists());
}

#[test]
fn global_transaction_rejects_destinations_inside_managed_projects() {
    let managed = tempfile::tempdir().unwrap();
    let standalone = tempfile::tempdir().unwrap();
    let destination = managed.path().join(".kiro/skills/demo");
    std::fs::write(managed.path().join(crate::manifest::MANIFEST_FILE), "").unwrap();

    let result = apply_standalone_global(
        standalone.path(),
        vec![Operation::file(&destination, b"global".to_vec())],
        true,
    );

    assert!(result.is_err());
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn global_transaction_rejects_managed_projects_behind_symlinked_roots() {
    let managed = tempfile::tempdir().unwrap();
    let standalone = tempfile::tempdir().unwrap();
    let aliases = tempfile::tempdir().unwrap();
    let alias = aliases.path().join("managed-alias");
    std::fs::write(managed.path().join(crate::manifest::MANIFEST_FILE), "").unwrap();
    std::os::unix::fs::symlink(managed.path(), &alias).unwrap();
    let destination = alias.join(".kiro/skills/demo");

    let result = apply_standalone_global(
        standalone.path(),
        vec![Operation::file(&destination, b"global".to_vec())],
        true,
    );

    assert!(result.is_err());
    assert!(!managed.path().join(".kiro/skills/demo").exists());
}

#[test]
fn global_transaction_rechecks_standalone_root_before_writing() {
    let project = tempfile::tempdir().unwrap();
    let destination_root = tempfile::tempdir().unwrap();
    let destination = destination_root.path().join("skills/demo");
    std::fs::write(project.path().join(crate::manifest::MANIFEST_FILE), "").unwrap();

    let result = apply_standalone_global(
        project.path(),
        vec![Operation::file(&destination, b"demo".to_vec())],
        true,
    );

    assert!(result.is_err());
    assert!(!destination.exists());
}

#[test]
fn recovery_stops_on_unknown_manual_content() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("a"), "old").unwrap();
    set_failure_phase("ARU_TEST_CRASH_AFTER", Some(1));
    assert!(apply(project.path(), vec![Operation::file("a", b"new".to_vec())]).is_err());
    set_failure_phase("ARU_TEST_CRASH_AFTER", None);
    std::fs::write(project.path().join("a"), "manual").unwrap();
    assert!(recover_if_needed(project.path()).is_err());
    assert_eq!(std::fs::read(project.path().join("a")).unwrap(), b"manual");
    assert!(project.path().join(JOURNAL_FILE).exists());
}

#[test]
fn committed_journal_rolls_forward_and_cleans_backup() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join(".aru")).unwrap();
    std::fs::write(project.path().join("a"), "new").unwrap();
    std::fs::write(project.path().join(".backup"), "old").unwrap();
    let journal = Journal {
        version: 1,
        phase: "committed".into(),
        root: None,
        entries: vec![JournalEntry {
            destination: "a".into(),
            stage: None,
            backup: Some(".backup".into()),
            old_digest: Some("sha256:old".into()),
            new_digest: path_digest(&project.path().join("a")).unwrap(),
            applied: true,
        }],
    };
    write_journal(&project.path().join(JOURNAL_FILE), &journal).unwrap();
    assert!(recover_if_needed(project.path()).unwrap());
    assert_eq!(std::fs::read(project.path().join("a")).unwrap(), b"new");
    assert!(!project.path().join(".backup").exists());
}

#[test]
fn crash_between_backup_and_stage_rename_restores_old_content() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join(".aru")).unwrap();
    std::fs::write(project.path().join("a"), "old").unwrap();
    let old_digest = path_digest(&project.path().join("a")).unwrap();
    let backup = project.path().join(".backup");
    std::fs::rename(project.path().join("a"), &backup).unwrap();
    let journal = Journal {
        version: 1,
        phase: "applying".into(),
        root: None,
        entries: vec![JournalEntry {
            destination: "a".into(),
            stage: None,
            backup: Some(".backup".into()),
            old_digest,
            new_digest: Some("sha256:new".into()),
            applied: false,
        }],
    };
    write_journal(&project.path().join(JOURNAL_FILE), &journal).unwrap();
    assert!(recover_if_needed(project.path()).unwrap());
    assert_eq!(std::fs::read(project.path().join("a")).unwrap(), b"old");
}

#[test]
fn v1_transaction_fixture_has_stable_round_trip() {
    let fixture = include_str!("../../tests/fixtures/contracts/transaction.toml");
    let journal: Journal = toml::from_str(fixture).unwrap();
    assert_eq!(toml::to_string_pretty(&journal).unwrap(), fixture);
}

#[test]
fn v2_standalone_transaction_fixture_has_stable_round_trip() {
    let fixture = include_str!("../../tests/fixtures/contracts/standalone-transaction.toml");
    let journal: Journal = toml::from_str(fixture).unwrap();
    assert_eq!(toml::to_string_pretty(&journal).unwrap(), fixture);
}

#[cfg(unix)]
#[test]
fn escaping_parent_symlink_is_rejected_before_staging() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join(".agents")).unwrap();
    let result = apply(
        project.path(),
        vec![Operation::file(".agents/skills/demo", b"unsafe".to_vec())],
    );
    assert!(result.is_err());
    assert!(!outside.path().join("skills").exists());
}

#[cfg(unix)]
#[test]
fn local_standalone_rejects_an_escaping_parent_symlink() {
    let project = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), project.path().join(".agents")).unwrap();

    let result = apply_standalone(
        project.path(),
        vec![Operation::file(".agents/skills/demo", b"unsafe".to_vec())],
        true,
    );

    assert!(result.is_err());
    assert!(!outside.path().join("skills").exists());
}
