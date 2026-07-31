use super::*;

#[test]
fn mutation_preserves_unrelated_comments() {
    let text = "# heading\nfuture = 1\n\n[project]\ntargets = [\"codex\"] # keep\n\n[skills]\n# package note\n\"owner/repo\" = { include = [\"old\"] }\n\n[custom]\nanswer = 42\n";
    let doc = text.parse::<DocumentMut>().unwrap();
    let mut document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc,
    };
    document.set_skill(
        "owner/repo",
        &SkillRequirement {
            include: vec!["new".into()],
            ..SkillRequirement::default()
        },
    );
    let output = String::from_utf8(document.bytes()).unwrap();
    assert!(output.contains("# heading"));
    assert!(output.contains("# package note"));
    assert!(output.contains("[custom]\nanswer = 42"));
    assert!(output.contains("include = [\"new\"]"));
}

#[test]
fn optional_tables_are_created_only_when_mutated() {
    let text = "# keep\nfuture = 1\n\n[project]\ntargets = [\"codex\"]\n\n[custom]\nanswer = 42\n";
    let document = || ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };

    let mut instructions = document();
    instructions.set_instruction_sources(&[InstructionSource {
        files: vec!["AGENTS.md".into()],
        exclude: Vec::new(),
        scope: Some(InstructionSourceScope::SourceDirectory),
        apply_to: Vec::new(),
        targets: Vec::new(),
    }]);
    let instructions_output = String::from_utf8(instructions.bytes()).unwrap();
    assert!(instructions_output.contains("[[instructions.sources]]"));
    assert!(!instructions_output.contains("[skills]"));
    assert!(!instructions_output.contains("[mcp]"));
    assert!(instructions_output.contains("[custom]\nanswer = 42"));
    assert_eq!(
        instructions.manifest().unwrap().instructions.sources.len(),
        1
    );

    let mut skills = document();
    skills.set_skill("owner/repo", &SkillRequirement::default());
    let skills_output = String::from_utf8(skills.bytes()).unwrap();
    assert!(skills_output.contains("[skills]"));
    assert!(!skills_output.contains("[instructions]"));
    assert!(!skills_output.contains("[mcp]"));
    assert!(skills_output.contains("[custom]\nanswer = 42"));
    assert_eq!(skills.manifest().unwrap().skills.len(), 1);

    let mut mcp = document();
    mcp.set_mcp(
        "demo",
        &McpRequirement {
            registry: None,
            server: None,
            version: None,
            transport: None,
            package_registry: None,
            url: None,
            command: Some("demo-mcp".into()),
            args: Vec::new(),
            env_vars: Vec::new(),
            env_http_headers: BTreeMap::new(),
            bearer_token_env: None,
            targets: None,
        },
    );
    let mcp_output = String::from_utf8(mcp.bytes()).unwrap();
    assert!(mcp_output.contains("[mcp.demo]"));
    assert!(!mcp_output.contains("[instructions]"));
    assert!(!mcp_output.contains("[skills]"));
    assert!(mcp_output.contains("[custom]\nanswer = 42"));
    assert_eq!(mcp.manifest().unwrap().mcp.len(), 1);
}

#[test]
fn mcp_environment_references_round_trip_and_normalize() {
    let text = "[project]\ntargets = [\"codex\"]\n\n[mcp.demo]\ncommand = \"demo-mcp\"\nenv-vars = [\"Z_TOKEN\", \"A_TOKEN\"]\n";
    let mut document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };
    let mut requirement = document.manifest().unwrap().mcp["demo"].clone();
    requirement.normalize();
    assert_eq!(requirement.env_vars, ["A_TOKEN", "Z_TOKEN"]);

    requirement = McpRequirement {
        registry: None,
        server: None,
        version: None,
        transport: None,
        package_registry: None,
        url: Some("https://example.com/mcp".into()),
        command: None,
        args: Vec::new(),
        env_vars: Vec::new(),
        env_http_headers: BTreeMap::from([
            ("X-API-Key".into(), "DOCS_API_KEY".into()),
            ("X-Tenant-ID".into(), "DOCS_TENANT".into()),
        ]),
        bearer_token_env: None,
        targets: None,
    };
    document.set_mcp("demo", &requirement);
    let output = String::from_utf8(document.bytes()).unwrap();
    assert!(output.contains("[mcp.demo.env-http-headers]"));
    assert!(output.contains("X-API-Key = \"DOCS_API_KEY\""));
    assert_eq!(
        document.manifest().unwrap().mcp["demo"].env_http_headers,
        requirement.env_http_headers
    );
}

#[test]
fn mcp_environment_references_reject_ambiguous_or_unsafe_inputs() {
    let invalid = [
        "[mcp.demo]\nurl = \"https://example.com/mcp\"\nenv-vars = [\"TOKEN\"]\n",
        "[mcp.demo]\ncommand = \"demo\"\n[mcp.demo.env-http-headers]\nX-Key = \"TOKEN\"\n",
        "[mcp.demo]\ncommand = \"demo\"\nenv-vars = [\"TOKEN\", \"TOKEN\"]\n",
        "[mcp.demo]\ncommand = \"demo\"\nenv-vars = [\"lowercase\"]\n",
        "[mcp.demo]\nurl = \"https://example.com/mcp\"\n[mcp.demo.env-http-headers]\n\"Bad Header\" = \"TOKEN\"\n",
        "[mcp.demo]\nurl = \"https://example.com/mcp\"\nbearer-token-env = \"TOKEN\"\n[mcp.demo.env-http-headers]\nAuthorization = \"OTHER_TOKEN\"\n",
        "[mcp.demo]\nurl = \"https://example.com/mcp\"\n[mcp.demo.env-http-headers]\nAuthorization = \"TOKEN\"\nauthorization = \"OTHER_TOKEN\"\n",
    ];
    for mcp in invalid {
        let text = format!("[project]\ntargets = [\"codex\"]\n\n{mcp}");
        let document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: text.parse().unwrap(),
        };
        assert!(document.manifest().is_err(), "accepted invalid MCP:\n{mcp}");
    }
}

#[test]
fn removing_from_absent_optional_tables_is_a_noop() {
    let text = "# keep\nfuture = 1\n\n[project]\ntargets = [\"codex\"]\n\n[custom]\nanswer = 42\n";
    let mut document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };

    document.remove_skill("missing");
    document.remove_mcp("missing");

    assert_eq!(document.bytes(), text.as_bytes());
    assert!(document.manifest().is_ok());
}

#[test]
fn target_mutation_preserves_the_key_comment_and_unrelated_content() {
    let text = "# heading\nfuture = 1\n\n[project]\ntargets = [\"codex\"] # why this set exists\n\n[custom]\nanswer = 42\n";
    let mut document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };

    document.set_targets(&[Target::Codex, Target::Claude]);

    let output = String::from_utf8(document.bytes()).unwrap();
    assert!(output.contains("targets = [\"codex\", \"claude\"] # why this set exists"));
    assert!(output.starts_with("# heading\nfuture = 1"));
    assert!(output.contains("[custom]\nanswer = 42"));
}

#[test]
fn branch_fixture_round_trips_without_manifest_schema() {
    let fixture = include_str!("../../tests/fixtures/contracts/aru-branch.toml");
    let document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: fixture.parse().unwrap(),
    };
    let manifest = document.manifest().unwrap();
    assert_eq!(
        manifest.skills["owner/repository"].branch.as_deref(),
        Some("main")
    );
    assert_eq!(document.bytes(), fixture.as_bytes());
    assert!(!fixture.contains("schema ="));
    assert!(
        !String::from_utf8(ManifestDocument::new(&[Target::Codex]).bytes())
            .unwrap()
            .contains("schema =")
    );
}

#[test]
fn branch_mutation_preserves_comments_and_reference_kinds_are_exclusive() {
    let text = "# keep\nfuture = 999\n\n[project]\ntargets = [\"codex\"]\n\n[skills]\n";
    let mut document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };
    document.set_skill(
        "owner/repo",
        &SkillRequirement {
            branch: Some("main".into()),
            ..SkillRequirement::default()
        },
    );
    assert!(document.manifest().is_ok());
    let output = String::from_utf8(document.bytes()).unwrap();
    assert!(output.starts_with("# keep\nfuture = 999"));
    assert!(output.contains("branch = \"main\""));

    let invalid = SkillRequirement {
        version: Some("1.0.0".into()),
        branch: Some("main".into()),
        ..SkillRequirement::default()
    };
    assert!(invalid.validate("owner/repo").is_err());
}

#[test]
fn instruction_sources_parse_and_validate_scope_and_targets() {
    let text = r#"
[project]
targets = ["codex", "claude", "copilot", "pi", "opencode"]

[[instructions.sources]]
files = ["AGENTS.md", "src/**/AGENTS.md"]
exclude = ["target/**"]
scope = "source-directory"

[[instructions.sources]]
files = ["docs/rust.md"]
apply-to = ["**/*.rs"]
targets = ["claude", "copilot"]
"#;
    let document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };
    let manifest = document.manifest().unwrap();
    assert_eq!(manifest.instructions.sources.len(), 2);
    assert_eq!(
        manifest.instructions.sources[1].targets,
        [Target::Claude, Target::Copilot]
    );
    assert_eq!(document.bytes(), text.as_bytes());
}

#[test]
fn instruction_source_rejects_ambiguous_scope_and_undeclared_target() {
    let source = InstructionSource {
        files: vec!["AGENTS.md".into()],
        exclude: Vec::new(),
        scope: Some(InstructionSourceScope::SourceDirectory),
        apply_to: vec!["**/*.rs".into()],
        targets: Vec::new(),
    };
    assert!(
        source
            .validate(&[Target::Codex, Target::Claude])
            .unwrap_err()
            .to_string()
            .contains("exactly one")
    );
    let source = InstructionSource {
        scope: None,
        apply_to: vec!["**/*.rs".into()],
        targets: vec![Target::Copilot],
        ..source
    };
    assert!(
        source
            .validate(&[Target::Claude])
            .unwrap_err()
            .to_string()
            .contains("not declared")
    );
}

#[test]
fn instruction_mutation_preserves_unrelated_manifest_content() {
    let text = "# keep\n[project]\ntargets = [\"claude\"]\n\n[instructions]\n# replace only sources\n\n[custom]\nanswer = 42\n";
    let mut document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };
    document.set_instruction_sources(&[InstructionSource {
        files: vec!["AGENTS.md".into()],
        exclude: Vec::new(),
        scope: Some(InstructionSourceScope::SourceDirectory),
        apply_to: Vec::new(),
        targets: Vec::new(),
    }]);
    let output = String::from_utf8(document.bytes()).unwrap();
    assert!(output.starts_with("# keep"));
    assert!(output.contains("# replace only sources"));
    assert!(output.contains("[custom]\nanswer = 42"));
    assert!(output.contains("files = [\"AGENTS.md\"]"));
    assert!(document.manifest().is_ok());
}

#[test]
fn dependency_targets_are_non_empty_capable_project_subsets() {
    let valid = r#"
[project]
targets = ["codex", "claude"]

[skills]
"owner/repo" = { include = ["*"], targets = ["codex"] }

[mcp.docs]
url = "https://example.com/mcp"
targets = ["claude"]
"#;
    let document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: valid.parse().unwrap(),
    };
    let manifest = document.manifest().unwrap();
    assert_eq!(
        manifest.skills["owner/repo"].targets,
        Some(vec![Target::Codex])
    );
    assert_eq!(manifest.mcp["docs"].targets, Some(vec![Target::Claude]));

    for invalid in [
        valid.replace("targets = [\"codex\"]", "targets = []"),
        valid.replace("targets = [\"codex\"]", "targets = [\"copilot\"]"),
        valid.replace("targets = [\"codex\"]", "targets = [\"codex\", \"codex\"]"),
    ] {
        let document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: invalid.parse().unwrap(),
        };
        assert!(document.manifest().is_err());
    }
}

#[test]
fn package_and_trust_mutation_preserve_unrelated_manifest_content() {
    let text = "# keep\n[project]\ntargets = [\"codex\"]\n\n[custom]\nanswer = 42\n";
    let mut document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: text.parse().unwrap(),
    };
    document.set_package(
        "owner/kit",
        &PackageRequirement {
            version: Some("^1.0".into()),
            targets: Some(vec![Target::Codex]),
            ..PackageRequirement::default()
        },
    );
    document.set_package_trust(
        "owner/kit",
        &PackageTrust {
            mcp: vec!["docs".into()],
        },
    );
    let output = String::from_utf8(document.bytes()).unwrap();
    assert!(output.starts_with("# keep"));
    assert!(output.contains("[custom]\nanswer = 42"));
    assert!(output.contains("[packages]"));
    assert!(output.contains("version = \"^1.0\""));
    assert!(output.contains("[package-trust.\"owner/kit\"]"));
    assert!(document.manifest().is_ok());

    document.remove_package("owner/kit");
    document.remove_package_trust("owner/kit");
    assert!(document.manifest().unwrap().packages.is_empty());
    assert!(document.manifest().unwrap().package_trust.is_empty());
}

#[test]
fn manifest_fixture_parses_and_preserves_comments() {
    let fixture = include_str!("../../tests/fixtures/contracts/aru.toml");
    let document = ManifestDocument {
        path: PathBuf::from("aru.toml"),
        doc: fixture.parse().unwrap(),
    };
    let manifest = document.manifest().unwrap();
    assert_eq!(manifest.project.targets.len(), 2);
    assert_eq!(
        manifest.skills["owner/repository"].paths["writing-plans"],
        "skills/writing-plans"
    );
    assert_eq!(manifest.mcp["demo"].env_vars, ["DEMO_TOKEN"]);
    assert_eq!(
        manifest.mcp["docs"].env_http_headers["X-API-Key"],
        "DOCS_API_KEY"
    );
    assert_eq!(document.bytes(), fixture.as_bytes());
}
