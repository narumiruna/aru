use super::*;

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
