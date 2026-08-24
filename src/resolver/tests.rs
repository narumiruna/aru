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
                env_vars: Vec::new(),
                env_http_headers: BTreeMap::new(),
                bearer_token_env: None,
                targets: None,
            },
        )]),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
        plugins: BTreeMap::new(),
        plugin_trust: BTreeMap::new(),
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
fn single_mcp_resolution_reuses_direct_candidate_and_target_validation() {
    let targets = [
        Target::Codex,
        Target::Claude,
        Target::Copilot,
        Target::Opencode,
    ];
    let requirement = McpRequirement {
        registry: None,
        server: None,
        version: None,
        transport: None,
        package_registry: None,
        url: None,
        command: Some("uvx".into()),
        args: vec!["demo@1.0.0".into()],
        env_vars: vec!["DEMO_TOKEN".into()],
        env_http_headers: BTreeMap::new(),
        bearer_token_env: None,
        targets: None,
    };
    let server = resolve_mcp_requirement("demo", &requirement, &targets, true).unwrap();
    assert_eq!(server.version, "direct");
    assert_eq!(
        server
            .targets
            .iter()
            .map(|target| target.target)
            .collect::<Vec<_>>(),
        targets
    );
    assert!(server.targets.iter().all(|target| {
        target.command.as_deref() == Some("uvx")
            && target.args == ["demo@1.0.0"]
            && target.env_vars == ["DEMO_TOKEN"]
    }));

    let error = resolve_mcp_requirement("demo", &requirement, &[Target::Pi], true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("unsupported by pi"));
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
        plugins: BTreeMap::new(),
        plugin_trust: BTreeMap::new(),
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
fn mcp_environment_hash_normalizes_order_and_tracks_references() {
    let mut manifest = Manifest {
        project: crate::manifest::Project {
            targets: vec![Target::Codex],
        },
        instructions: crate::manifest::Instructions::default(),
        skills: BTreeMap::new(),
        mcp: BTreeMap::from([(
            "demo".into(),
            McpRequirement {
                registry: None,
                server: None,
                version: None,
                transport: None,
                package_registry: None,
                url: None,
                command: Some("demo-mcp".into()),
                args: Vec::new(),
                env_vars: vec!["Z_TOKEN".into(), "A_TOKEN".into()],
                env_http_headers: BTreeMap::new(),
                bearer_token_env: None,
                targets: None,
            },
        )]),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
        plugins: BTreeMap::new(),
        plugin_trust: BTreeMap::new(),
    };
    let empty = BTreeMap::new();
    let first = package_input_hash(&manifest, &empty, &empty).unwrap();
    manifest.mcp.get_mut("demo").unwrap().env_vars.reverse();
    assert_eq!(
        first,
        package_input_hash(&manifest, &empty, &empty).unwrap()
    );
    manifest.mcp.get_mut("demo").unwrap().env_vars[0] = "B_TOKEN".into();
    assert_ne!(
        first,
        package_input_hash(&manifest, &empty, &empty).unwrap()
    );
}

#[test]
fn locked_pypi_candidate_replays_offline_to_every_mcp_target() {
    let temporary = tempfile::tempdir().unwrap();
    let targets = vec![
        Target::Codex,
        Target::Claude,
        Target::Copilot,
        Target::Opencode,
    ];
    let requirement = McpRequirement {
        registry: Some(crate::registry::DEFAULT_REGISTRY.into()),
        server: Some("io.example/weather".into()),
        version: Some("=0.5.0".into()),
        transport: Some("stdio".into()),
        package_registry: Some("pypi".into()),
        url: None,
        command: None,
        args: Vec::new(),
        env_vars: Vec::new(),
        env_http_headers: BTreeMap::new(),
        bearer_token_env: None,
        targets: None,
    };
    let manifest = Manifest {
        project: crate::manifest::Project {
            targets: targets.clone(),
        },
        instructions: crate::manifest::Instructions::default(),
        skills: BTreeMap::new(),
        mcp: BTreeMap::from([("weather".into(), requirement.clone())]),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
        plugins: BTreeMap::new(),
        plugin_trust: BTreeMap::new(),
    };
    let candidate = crate::registry::ResolvedCandidate {
        kind: "package".into(),
        transport: "stdio".into(),
        command: Some("uvx".into()),
        args: vec!["weather-mcp@0.5.0".into()],
        env_vars: vec!["WEATHER_API_KEY".into()],
        env_http_headers: BTreeMap::new(),
        bearer_token_env: None,
        url: None,
        package: Some(crate::lockfile::LockedMcpPackage {
            registry: "pypi".into(),
            identifier: "weather-mcp".into(),
            version: "0.5.0".into(),
        }),
    };
    let mut lock = Lockfile::empty();
    lock.package_input_hash =
        package_input_hash(&manifest, &BTreeMap::new(), &BTreeMap::new()).unwrap();
    lock.mcp_servers.push(McpServer {
        name: "weather".into(),
        origin: None,
        registry: Some(crate::registry::DEFAULT_REGISTRY.into()),
        server_id: "io.example/weather".into(),
        requirement: canonical_json_digest(&normalized_mcp(&requirement)).unwrap(),
        version: "0.5.0".into(),
        metadata_sha256: "sha256:metadata".into(),
        targets: targets_from_candidate(&targets, &candidate, None).unwrap(),
    });
    lock.normalize();
    lock.projection_baselines = baselines(&lock, &[]).unwrap();
    lock.projection_input_hash = projection_input_hash(&lock, &targets).unwrap();
    lock.normalize();

    let resolved = resolve(
        temporary.path(),
        &manifest,
        ResolveOptions {
            previous: Some(&lock),
            locked: true,
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
    .unwrap();
    let server = &resolved.lock.mcp_servers[0];
    assert_eq!(
        server
            .targets
            .iter()
            .map(|target| target.target)
            .collect::<Vec<_>>(),
        targets
    );
    assert!(server.targets.iter().all(|target| {
        target.command.as_deref() == Some("uvx")
            && target.args == ["weather-mcp@0.5.0"]
            && target.env_vars == ["WEATHER_API_KEY"]
            && target.package.as_ref().unwrap().registry == "pypi"
    }));
}

#[test]
fn projection_hash_sorts_target_set() {
    let lock = Lockfile::empty();
    let forward = projection_input_hash(&lock, &[Target::Codex, Target::Claude]).unwrap();
    let reverse = projection_input_hash(&lock, &[Target::Claude, Target::Codex]).unwrap();
    assert_eq!(forward, reverse);
}
