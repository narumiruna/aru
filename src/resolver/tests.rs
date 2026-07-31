use std::process::Command;

use super::*;

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

#[test]
fn offline_registry_resolution_fails_before_http_resolution() {
    let temporary = tempfile::tempdir().unwrap();
    let manifest = Manifest {
        project: crate::manifest::Project {
            targets: vec![Target::Codex],
        },
        instructions: crate::manifest::Instructions::default(),
        skills: BTreeMap::new(),
        mcp: BTreeMap::from([(
            "docs".into(),
            McpRequirement {
                registry: Some("https://127.0.0.1:9".into()),
                server: Some("io.example/docs".into()),
                version: None,
                transport: Some("stdio".into()),
                package_registry: Some("npm".into()),
                url: None,
                command: None,
                args: Vec::new(),
                bearer_token_env: None,
                targets: None,
            },
        )]),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
    };
    let error = resolve(
        temporary.path(),
        &manifest,
        ResolveOptions {
            previous: None,
            locked: false,
            offline: true,
            materialize_skills: true,
            update_skills: &BTreeSet::new(),
            update_mcp: &BTreeSet::new(),
            update_packages: &BTreeSet::new(),
            precise_packages: &BTreeMap::new(),
            dry_run: false,
            skill_hints: &BTreeMap::new(),
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("offline mode"));
    assert!(error.contains("docs"));
}

#[test]
fn package_hash_excludes_targets() {
    let source = GitSource {
        identity: "git+https://example.com/a.git".into(),
        fetch: "https://example.com/a.git".into(),
        repository_name: "a".into(),
    };
    let mut sources = BTreeMap::new();
    sources.insert("source".into(), source);
    let mut manifest = Manifest {
        project: crate::manifest::Project {
            targets: vec![Target::Codex],
        },
        instructions: crate::manifest::Instructions::default(),
        skills: BTreeMap::from([("source".into(), SkillRequirement::default())]),
        mcp: BTreeMap::new(),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
    };
    let package_sources = BTreeMap::new();
    let first = package_input_hash(&manifest, &sources, &package_sources).unwrap();
    manifest.project.targets.push(Target::Claude);
    assert_eq!(
        first,
        package_input_hash(&manifest, &sources, &package_sources).unwrap()
    );
    manifest.skills.get_mut("source").unwrap().targets = Some(vec![Target::Codex]);
    assert_eq!(
        first,
        package_input_hash(&manifest, &sources, &package_sources).unwrap()
    );

    manifest.skills.get_mut("source").unwrap().include = vec!["zeta".into(), "alpha".into()];
    let selectors = package_input_hash(&manifest, &sources, &package_sources).unwrap();
    manifest.skills.get_mut("source").unwrap().include.reverse();
    assert_eq!(
        selectors,
        package_input_hash(&manifest, &sources, &package_sources).unwrap()
    );

    let package_source = GitSource {
        identity: "git+https://example.com/kit.git".into(),
        fetch: "https://example.com/kit.git".into(),
        repository_name: "kit".into(),
    };
    let package_sources = BTreeMap::from([("kit".into(), package_source)]);
    manifest.packages.insert(
        "kit".into(),
        PackageRequirement {
            version: Some("^1.0".into()),
            targets: Some(vec![Target::Codex]),
            ..PackageRequirement::default()
        },
    );
    let package_hash = package_input_hash(&manifest, &sources, &package_sources).unwrap();
    manifest.packages.get_mut("kit").unwrap().targets = Some(vec![Target::Claude]);
    manifest.package_trust.insert(
        "kit".into(),
        crate::manifest::PackageTrust {
            mcp: vec!["docs".into()],
        },
    );
    assert_eq!(
        package_hash,
        package_input_hash(&manifest, &sources, &package_sources).unwrap()
    );
}

#[test]
fn projection_hash_sorts_target_set() {
    let lock = Lockfile::empty();
    let forward = projection_input_hash(&lock, &[Target::Codex, Target::Claude]).unwrap();
    let reverse = projection_input_hash(&lock, &[Target::Claude, Target::Codex]).unwrap();
    assert_eq!(forward, reverse);
}
