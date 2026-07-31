use std::process::Command;

use super::*;
use crate::resolver::{ResolveOptions, resolve};

fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn write_skill(repository: &Path, name: &str) {
    let directory = repository.join("skills").join(name);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Test\n---\n# Test\n"),
    )
    .unwrap();
}

#[test]
fn inspection_reuses_locks_and_hint_pins_the_previewed_revision() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&repository).unwrap();
    std::fs::create_dir(&project).unwrap();
    git(&repository, &["init", "--quiet"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "commit.gpgsign", "false"]);
    write_skill(&repository, "zeta");
    write_skill(&repository, "alpha");
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "first"]);
    git(&repository, &["tag", "1.0.0"]);
    let first_revision = git(&repository, &["rev-parse", "HEAD"]);

    let requirement = SkillRequirement {
        version: Some("=1.0.0".into()),
        ..SkillRequirement::default()
    };
    let manifest = Manifest {
        project: crate::manifest::Project {
            targets: vec![Target::Codex],
        },
        instructions: crate::manifest::Instructions::default(),
        skills: BTreeMap::from([(
            repository.to_string_lossy().into_owned(),
            requirement.clone(),
        )]),
        mcp: BTreeMap::new(),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
    };
    let empty_hints = BTreeMap::new();
    let initial = resolve(
        &project,
        &manifest,
        ResolveOptions {
            previous: None,
            locked: false,
            offline: false,
            materialize_skills: true,
            update_skills: &BTreeSet::new(),
            update_mcp: &BTreeSet::new(),
            update_packages: &BTreeSet::new(),
            precise_packages: &BTreeMap::new(),
            dry_run: false,
            skill_hints: &empty_hints,
        },
    )
    .unwrap();

    std::fs::write(repository.join("changed"), "second").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "second"]);
    let second_revision = git(&repository, &["rev-parse", "HEAD"]);
    git(&repository, &["tag", "--force", "1.0.0"]);

    let inspection = inspect_skill_source(
        &project,
        &repository.to_string_lossy(),
        &requirement,
        Some(&initial.lock),
        false,
        false,
    )
    .unwrap();
    assert_eq!(inspection.revision, first_revision);
    assert_eq!(
        inspection
            .candidates
            .iter()
            .map(|candidate| candidate.name.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "zeta"]
    );

    let hints = BTreeMap::from([(inspection.source.clone(), inspection.hint())]);
    let pinned = resolve(
        &project,
        &manifest,
        ResolveOptions {
            previous: None,
            locked: false,
            offline: false,
            materialize_skills: true,
            update_skills: &BTreeSet::new(),
            update_mcp: &BTreeSet::new(),
            update_packages: &BTreeSet::new(),
            precise_packages: &BTreeMap::new(),
            dry_run: false,
            skill_hints: &hints,
        },
    )
    .unwrap();
    assert_eq!(pinned.lock.skill_packages[0].revision, first_revision);

    let changed_requirement = SkillRequirement {
        version: None,
        rev: Some(second_revision.clone()),
        ..requirement.clone()
    };
    let changed = inspect_skill_source(
        &project,
        &repository.to_string_lossy(),
        &changed_requirement,
        Some(&initial.lock),
        false,
        false,
    )
    .unwrap();
    assert_eq!(changed.revision, second_revision);

    let invalid = SkillResolutionHint {
        requirement: "version:>=9.0.0".into(),
        ..inspection.hint()
    };
    assert!(validate_skill_hint(&invalid, &inspection.source, &requirement).is_err());
}

#[test]
fn branch_inspection_reuses_lock_update_moves_and_hint_pins_head() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&repository).unwrap();
    std::fs::create_dir(&project).unwrap();
    git(&repository, &["init", "--quiet"]);
    git(&repository, &["config", "user.email", "test@example.com"]);
    git(&repository, &["config", "user.name", "Test"]);
    git(&repository, &["config", "commit.gpgsign", "false"]);
    write_skill(&repository, "alpha");
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "first"]);
    git(&repository, &["branch", "live"]);
    let first_revision = git(&repository, &["rev-parse", "HEAD"]);

    let requirement = SkillRequirement {
        branch: Some("live".into()),
        ..SkillRequirement::default()
    };
    let manifest = Manifest {
        project: crate::manifest::Project {
            targets: vec![Target::Codex],
        },
        instructions: crate::manifest::Instructions::default(),
        skills: BTreeMap::from([(
            repository.to_string_lossy().into_owned(),
            requirement.clone(),
        )]),
        mcp: BTreeMap::new(),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
    };
    let empty_hints = BTreeMap::new();
    let first = resolve(
        &project,
        &manifest,
        ResolveOptions {
            previous: None,
            locked: false,
            offline: false,
            materialize_skills: true,
            update_skills: &BTreeSet::new(),
            update_mcp: &BTreeSet::new(),
            update_packages: &BTreeSet::new(),
            precise_packages: &BTreeMap::new(),
            dry_run: false,
            skill_hints: &empty_hints,
        },
    )
    .unwrap();
    assert_eq!(first.lock.skill_packages[0].requirement, "branch:live");
    assert_eq!(first.lock.skill_packages[0].version, "live");

    std::fs::write(repository.join("changed"), "second").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "second"]);
    git(&repository, &["branch", "--force", "live", "HEAD"]);
    let second_revision = git(&repository, &["rev-parse", "HEAD"]);

    let inspection = inspect_skill_source(
        &project,
        &repository.to_string_lossy(),
        &requirement,
        Some(&first.lock),
        false,
        false,
    )
    .unwrap();
    assert_eq!(inspection.revision, first_revision);
    let hints = BTreeMap::from([(inspection.source.clone(), inspection.hint())]);
    let pinned = resolve(
        &project,
        &manifest,
        ResolveOptions {
            previous: None,
            locked: false,
            offline: false,
            materialize_skills: true,
            update_skills: &BTreeSet::new(),
            update_mcp: &BTreeSet::new(),
            update_packages: &BTreeSet::new(),
            precise_packages: &BTreeMap::new(),
            dry_run: false,
            skill_hints: &hints,
        },
    )
    .unwrap();
    assert_eq!(pinned.lock.skill_packages[0].revision, first_revision);

    let source = first.lock.skill_packages[0].source.clone();
    let updated = resolve(
        &project,
        &manifest,
        ResolveOptions {
            previous: Some(&first.lock),
            locked: false,
            offline: false,
            materialize_skills: true,
            update_skills: &BTreeSet::from([source]),
            update_mcp: &BTreeSet::new(),
            update_packages: &BTreeSet::new(),
            precise_packages: &BTreeMap::new(),
            dry_run: false,
            skill_hints: &empty_hints,
        },
    )
    .unwrap();
    assert_eq!(updated.lock.skill_packages[0].revision, second_revision);
}
