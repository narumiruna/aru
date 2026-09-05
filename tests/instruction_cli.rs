use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn init(project: &Path, targets: &[&str]) {
    let mut command = aru(project);
    command.arg("init");
    for target in targets {
        command.args(["--target", target]);
    }
    command.assert().success();
}

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn create_skill_repository(repository: &Path) {
    std::fs::create_dir(repository).unwrap();
    for arguments in [
        &["init", "--quiet"][..],
        &["config", "user.email", "instructions@example.com"],
        &["config", "user.name", "instruction tests"],
        &["config", "commit.gpgsign", "false"],
    ] {
        assert!(
            Command::new("git")
                .current_dir(repository)
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
    }
    std::fs::create_dir_all(repository.join("skills/demo")).unwrap();
    std::fs::write(
        repository.join("skills/demo/SKILL.md"),
        "---\nname: demo\ndescription: Demo\n---\n# Demo\n",
    )
    .unwrap();
    for arguments in [
        &["add", "skills"][..],
        &["commit", "--quiet", "-m", "initial"],
        &["tag", "1.0.0"],
    ] {
        assert!(
            Command::new("git")
                .current_dir(repository)
                .args(arguments)
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
fn instruction_help_exposes_add_remove_list_and_merge_contract() {
    cargo_bin_cmd!("aru")
        .args(["instruction", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("remove"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("init").not());
    cargo_bin_cmd!("aru")
        .args(["instruction", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("[FILE]..."))
        .stdout(predicate::str::contains("--discover").not())
        .stdout(predicate::str::contains("-n, --dry-run"))
        .stdout(predicate::str::contains("--no-sync"))
        .stdout(predicate::str::contains("--merge"))
        .stdout(predicate::str::contains("--force"));
    cargo_bin_cmd!("aru")
        .args(["sync", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--check"))
        .stdout(predicate::str::contains("--merge"));
    cargo_bin_cmd!("aru")
        .args(["instruction", "add", "AGENTS.md", "--merge", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
    cargo_bin_cmd!("aru")
        .args(["instruction", "init"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[test]
fn instruction_add_list_and_remove_manage_declared_sources() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::write(project.join("AGENTS.md"), "# Root\n").unwrap();
    init(project, &["claude"]);

    aru(project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();
    aru(project)
        .args(["instruction", "list"])
        .assert()
        .success()
        .stdout("AGENTS.md\n");

    let manifest = std::fs::read(project.join("aru.toml")).unwrap();
    aru(project)
        .args(["instruction", "remove", "AGENTS.md", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Would unlock removed instruction AGENTS.md",
        ));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(project.join("CLAUDE.md").is_file());

    aru(project)
        .args(["instruction", "remove", "AGENTS.md"])
        .assert()
        .success();
    aru(project)
        .args(["instruction", "list"])
        .assert()
        .success()
        .stdout("");
    assert!(!project.join("CLAUDE.md").exists());
    assert_eq!(
        std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        "# Root\n"
    );
}

#[test]
fn instruction_add_accepts_manifest_without_optional_tables() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    let manifest = "# keep\n[project]\ntargets = [\"codex\"]\n";
    let instructions = "# Root instructions\n";
    std::fs::write(project.join("aru.toml"), manifest).unwrap();
    std::fs::write(project.join("AGENTS.md"), instructions).unwrap();

    aru(project)
        .args(["instruction", "add", "AGENTS.md", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would lock instruction AGENTS.md"));

    assert_eq!(
        std::fs::read_to_string(project.join("aru.toml")).unwrap(),
        manifest
    );
    assert_eq!(
        std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        instructions
    );
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru").exists());
    assert!(!project.join("CLAUDE.md").exists());
    assert!(!project.join(".codex").exists());
    assert!(!project.join(".mcp.json").exists());
}

#[test]
fn instruction_add_accepts_explicit_agents_files_and_projects_all_targets() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    let nested = project.join("src/api");
    std::fs::create_dir_all(&nested).unwrap();
    let root_content = "# Root instructions\n\nKeep the root source.\n";
    let nested_content = "# API instructions\n\nKeep the nested source.\n";
    std::fs::write(project.join("AGENTS.md"), root_content).unwrap();
    std::fs::write(nested.join("AGENTS.md"), nested_content).unwrap();

    aru(project)
        .args([
            "init", "--target", "agents", "--target", "codex", "--target", "claude", "--target",
            "copilot", "--target", "pi", "--target", "opencode",
        ])
        .assert()
        .success();
    aru(project)
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("agents\ncodex\nclaude\ncopilot\nopencode\npi\n");

    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();
    aru(project)
        .args([
            "instruction",
            "add",
            "AGENTS.md",
            "src/api/AGENTS.md",
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would lock instruction AGENTS.md"));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert!(!project.join("CLAUDE.md").exists());
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/state.toml").exists());

    aru(project)
        .args(["instruction", "add", "AGENTS.md", "src/api/AGENTS.md"])
        .assert()
        .success();

    let lock = aru::lockfile::Lockfile::load_optional(project)
        .unwrap()
        .unwrap();
    for source in &lock.instruction_sources {
        assert_eq!(
            source.targets,
            [
                aru::manifest::Target::Agents,
                aru::manifest::Target::Codex,
                aru::manifest::Target::Claude,
                aru::manifest::Target::Copilot,
                aru::manifest::Target::Opencode,
                aru::manifest::Target::Pi,
            ]
        );
    }

    assert_eq!(
        std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        root_content
    );
    assert_eq!(
        std::fs::read_to_string(nested.join("AGENTS.md")).unwrap(),
        nested_content
    );
    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("[[instructions.sources]]"));
    assert!(manifest.contains("\"AGENTS.md\""));
    assert!(manifest.contains("\"src/api/AGENTS.md\""));
    assert!(
        std::fs::read_to_string(project.join("CLAUDE.md"))
            .unwrap()
            .contains("@AGENTS.md")
    );
    assert!(
        std::fs::read_to_string(nested.join("CLAUDE.md"))
            .unwrap()
            .contains("@AGENTS.md")
    );
    assert!(
        std::fs::read_to_string(project.join(".github/copilot-instructions.md"))
            .unwrap()
            .contains("Keep the root source.")
    );
    let nested_copilot = project.join(".github/instructions/aru/src/api/AGENTS.instructions.md");
    let nested_projection = std::fs::read_to_string(nested_copilot).unwrap();
    assert!(nested_projection.contains("applyTo: \"src/api/**\""));
    assert!(nested_projection.contains("Keep the nested source."));
}

#[test]
fn merge_preserves_unmanaged_content_updates_blocks_and_cleans_owned_outputs() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::create_dir_all(project.join("src/api")).unwrap();
    std::fs::create_dir_all(project.join(".github")).unwrap();
    std::fs::write(project.join("AGENTS.md"), "# Root\n\nFirst version.\n").unwrap();
    std::fs::write(project.join("src/api/AGENTS.md"), "# API\n").unwrap();
    std::fs::write(project.join("CLAUDE.md"), "# Manual Claude\n").unwrap();
    std::fs::write(
        project.join(".github/copilot-instructions.md"),
        "# Manual Copilot\n",
    )
    .unwrap();
    init(project, &["claude", "copilot"]);

    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();
    aru(project)
        .args(["instruction", "add", "AGENTS.md", "src/api/AGENTS.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
        "# Manual Claude\n"
    );

    aru(project)
        .args([
            "instruction",
            "add",
            "AGENTS.md",
            "src/api/AGENTS.md",
            "--merge",
        ])
        .assert()
        .success();
    let claude = std::fs::read_to_string(project.join("CLAUDE.md")).unwrap();
    let copilot = std::fs::read_to_string(project.join(".github/copilot-instructions.md")).unwrap();
    assert!(claude.starts_with("# Manual Claude\n"));
    assert!(claude.contains("<!-- aru:instruction:start AGENTS.md -->"));
    assert!(copilot.starts_with("# Manual Copilot\n"));
    assert!(copilot.contains("First version."));

    std::fs::write(project.join("AGENTS.md"), "# Root\n\nSecond version.\n").unwrap();
    aru(project).arg("sync").assert().success();
    let copilot = std::fs::read_to_string(project.join(".github/copilot-instructions.md")).unwrap();
    assert!(copilot.contains("Second version."));
    assert!(!copilot.contains("First version."));
    assert!(copilot.starts_with("# Manual Copilot\n"));

    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(project)
        .args(["sync", "--locked"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Adopted instruction AGENTS.md"));

    let manifest_path = project.join("aru.toml");
    let mut manifest: toml_edit::DocumentMut = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .parse()
        .unwrap();
    manifest.remove("instructions");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    aru(project).arg("sync").assert().success();

    assert_eq!(
        std::fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
        "# Manual Claude\n\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.join(".github/copilot-instructions.md")).unwrap(),
        "# Manual Copilot\n\n"
    );
    assert!(
        !project
            .join(".github/instructions/aru/src/api/AGENTS.instructions.md")
            .exists()
    );
    assert_eq!(
        std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        "# Root\n\nSecond version.\n"
    );
    assert_eq!(
        std::fs::read_to_string(project.join("src/api/AGENTS.md")).unwrap(),
        "# API\n"
    );
}

#[test]
fn missing_state_preserves_removed_instruction_projection_for_review() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::write(project.join("AGENTS.md"), "# Root\n").unwrap();
    init(project, &["claude"]);
    aru(project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();
    let projected = std::fs::read(project.join("CLAUDE.md")).unwrap();
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    let manifest_path = project.join("aru.toml");
    let mut manifest: toml_edit::DocumentMut = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .parse()
        .unwrap();
    manifest.remove("instructions");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();

    aru(project)
        .arg("sync")
        .assert()
        .success()
        .stderr(predicate::str::contains("Warning"))
        .stderr(predicate::str::contains("CLAUDE.md"));
    assert_eq!(std::fs::read(project.join("CLAUDE.md")).unwrap(), projected);
}

#[test]
fn drifted_instruction_block_is_preserved_even_with_force() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::write(project.join("AGENTS.md"), "# Root\n").unwrap();
    init(project, &["claude"]);
    aru(project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();

    let path = project.join("CLAUDE.md");
    let drifted = std::fs::read_to_string(&path)
        .unwrap()
        .replace("@AGENTS.md", "@BROKEN.md");
    std::fs::write(&path, &drifted).unwrap();
    for args in [vec!["sync"], vec!["sync", "--force"]] {
        aru(project)
            .args(args)
            .assert()
            .failure()
            .stderr(predicate::str::contains("drift"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), drifted);
    }
}

#[test]
fn explicit_glob_rules_project_only_to_exact_scope_adapters() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::create_dir_all(project.join("docs/instructions")).unwrap();
    std::fs::write(
        project.join("docs/instructions/rust.md"),
        "# Rust\n\nAvoid unwrap.\n",
    )
    .unwrap();
    init(project, &["codex", "claude", "copilot"]);
    let manifest_path = project.join("aru.toml");
    let mut manifest = std::fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str(
        "\n[[instructions.sources]]\nfiles = [\"docs/instructions/rust.md\"]\napply-to = [\"**/*.rs\", \"crates/**\"]\ntargets = [\"claude\", \"copilot\"]\n",
    );
    std::fs::write(&manifest_path, manifest).unwrap();
    let claude_path = project.join(".claude/rules/aru/docs/instructions/rust.md");
    std::fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
    std::fs::write(&claude_path, "# Unmanaged rule\n").unwrap();
    aru(project)
        .args(["sync", "--merge"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));
    assert_eq!(
        std::fs::read_to_string(&claude_path).unwrap(),
        "# Unmanaged rule\n"
    );

    aru(project).args(["sync", "--force"]).assert().success();
    let claude =
        std::fs::read_to_string(project.join(".claude/rules/aru/docs/instructions/rust.md"))
            .unwrap();
    assert!(claude.contains("paths:\n  - \"**/*.rs\"\n  - \"crates/**\""));
    assert!(claude.contains("Avoid unwrap."));
    let copilot = std::fs::read_to_string(
        project.join(".github/instructions/aru/docs/instructions/rust.instructions.md"),
    )
    .unwrap();
    assert!(copilot.contains("applyTo: \"**/*.rs,crates/**\""));
    assert!(copilot.contains("Avoid unwrap."));

    let invalid = std::fs::read_to_string(&manifest_path).unwrap().replace(
        "targets = [\"claude\", \"copilot\"]",
        "targets = [\"codex\"]",
    );
    std::fs::write(&manifest_path, invalid).unwrap();
    aru(project)
        .arg("sync")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported by codex"));
}

#[test]
fn drifted_generated_instruction_file_is_never_updated_or_removed() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::create_dir(project.join("docs")).unwrap();
    std::fs::write(project.join("docs/rust.md"), "# Rust\n").unwrap();
    init(project, &["claude"]);
    let manifest_path = project.join("aru.toml");
    let mut manifest = std::fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str(
        "\n[[instructions.sources]]\nfiles = [\"docs/rust.md\"]\napply-to = [\"**/*.rs\"]\n",
    );
    std::fs::write(&manifest_path, manifest).unwrap();
    aru(project).arg("sync").assert().success();
    let output = project.join(".claude/rules/aru/docs/rust.md");
    let drifted = "# Manually replaced output\n";
    std::fs::write(&output, drifted).unwrap();
    std::fs::write(project.join("docs/rust.md"), "# Updated Rust\n").unwrap();
    aru(project)
        .args(["sync", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("drift"));
    assert_eq!(std::fs::read_to_string(&output).unwrap(), drifted);

    let mut manifest: toml_edit::DocumentMut = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .parse()
        .unwrap();
    manifest.remove("instructions");
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    aru(project)
        .args(["sync", "--force"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("drift"));
    assert_eq!(std::fs::read_to_string(&output).unwrap(), drifted);
}

#[test]
fn pi_supports_native_skills_but_rejects_mcp_projections() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let repository = temporary.path().join("repository");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["pi"]);
    aru(&project)
        .args([
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no configured target supports MCP projections",
        ));
    assert!(!project.join("aru.lock").exists());

    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();
    assert!(project.join(".pi/skills/demo").is_dir());
    assert!(!project.join(".agents/skills/demo").exists());
}

#[test]
fn lock_records_sources_without_outputs_and_locked_sync_replays_exact_content() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::write(project.join("AGENTS.md"), "# Root\n").unwrap();
    init(project, &["claude", "copilot"]);
    let manifest_path = project.join("aru.toml");
    let mut manifest = std::fs::read_to_string(&manifest_path).unwrap();
    manifest.push_str(
        "\n[[instructions.sources]]\nfiles = [\"AGENTS.md\"]\nscope = \"source-directory\"\n",
    );
    std::fs::write(&manifest_path, manifest).unwrap();

    aru(project).arg("lock").assert().success();
    assert!(!project.join("CLAUDE.md").exists());
    assert!(!project.join(".github/copilot-instructions.md").exists());
    let lock = aru::lockfile::Lockfile::load_optional(project)
        .unwrap()
        .unwrap();
    assert_eq!(lock.instruction_sources.len(), 1);
    assert_eq!(lock.instruction_sources[0].source, "AGENTS.md");
    assert_eq!(
        lock.projection_baselines
            .iter()
            .filter(|baseline| baseline.kind == "instruction")
            .count(),
        2
    );

    aru(project).args(["sync", "--locked"]).assert().success();
    let outputs = [
        std::fs::read(project.join("CLAUDE.md")).unwrap(),
        std::fs::read(project.join(".github/copilot-instructions.md")).unwrap(),
    ];
    std::fs::write(project.join("AGENTS.md"), "# Changed\n").unwrap();
    aru(project)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("stale for instruction sources"));
    assert_eq!(
        std::fs::read(project.join("CLAUDE.md")).unwrap(),
        outputs[0]
    );
    assert_eq!(
        std::fs::read(project.join(".github/copilot-instructions.md")).unwrap(),
        outputs[1]
    );
}

#[test]
fn instruction_add_is_idempotent_and_missing_file_is_actionable() {
    let temporary = tempfile::tempdir().unwrap();
    let empty = temporary.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    init(&empty, &["claude"]);
    aru(&empty)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("matched no files"));
    aru(&empty)
        .args(["instruction", "add", "**/AGENTS.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "requires exact AGENTS.md file paths",
        ));

    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("AGENTS.md"), "# Root\n").unwrap();
    init(&project, &["claude"]);
    aru(&project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();
    let lock = std::fs::read(project.join("aru.lock")).unwrap();
    let state = std::fs::read(project.join(".aru/state.toml")).unwrap();
    aru(&project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Project is synchronized."));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert_eq!(std::fs::read(project.join("aru.lock")).unwrap(), lock);
    assert_eq!(
        std::fs::read(project.join(".aru/state.toml")).unwrap(),
        state
    );
}

#[test]
fn force_takeover_is_explicit_in_dry_run_and_apply() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::write(project.join("AGENTS.md"), "# Root\n").unwrap();
    std::fs::write(project.join("CLAUDE.md"), "# Replace me\n").unwrap();
    init(project, &["claude"]);
    aru(project)
        .args(["instruction", "add", "AGENTS.md", "--force", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Would force replace instruction document (CLAUDE.md)",
        ));
    assert_eq!(
        std::fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
        "# Replace me\n"
    );
    aru(project)
        .args(["instruction", "add", "AGENTS.md", "--force"])
        .assert()
        .success();
    let claude = std::fs::read_to_string(project.join("CLAUDE.md")).unwrap();
    assert!(!claude.contains("Replace me"));
    assert!(claude.contains("@AGENTS.md"));
}

#[test]
fn target_changes_support_merge_and_deferred_instruction_projection() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::write(project.join("AGENTS.md"), "# Root\n").unwrap();
    init(project, &["codex"]);
    aru(project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();
    std::fs::write(project.join("CLAUDE.md"), "# Manual Claude\n").unwrap();
    std::fs::create_dir(project.join(".github")).unwrap();
    std::fs::write(
        project.join(".github/copilot-instructions.md"),
        "# Manual Copilot\n",
    )
    .unwrap();

    aru(project)
        .args(["target", "add", "claude", "copilot", "--merge", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would add target claude"))
        .stderr(predicate::str::contains(
            "Would create instruction AGENTS.md",
        ));
    assert_eq!(
        std::fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
        "# Manual Claude\n"
    );

    aru(project)
        .args(["target", "add", "claude", "copilot", "--merge", "--no-sync"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Target paths were not changed"));
    assert_eq!(
        std::fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
        "# Manual Claude\n"
    );
    aru(project).args(["sync", "--merge"]).assert().success();
    assert!(
        std::fs::read_to_string(project.join("CLAUDE.md"))
            .unwrap()
            .contains("@AGENTS.md")
    );

    aru(project)
        .args(["target", "remove", "claude"])
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(project.join("CLAUDE.md")).unwrap(),
        "# Manual Claude\n\n"
    );
    aru(project)
        .args(["target", "set", "codex"])
        .assert()
        .success();
    assert_eq!(
        std::fs::read_to_string(project.join(".github/copilot-instructions.md")).unwrap(),
        "# Manual Copilot\n\n"
    );
}

#[test]
fn adding_targets_expands_capable_projections_without_unlocking_packages() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let repository = temporary.path().join("repository");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["codex"]);
    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();
    aru(&project)
        .args([
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
        ])
        .assert()
        .success();
    let before = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();

    aru(&project)
        .args(["target", "add", "copilot", "pi", "opencode"])
        .assert()
        .success();
    let after = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    let before_skill = &before.skill_packages[0];
    let after_skill = &after.skill_packages[0];
    assert_eq!(before_skill.source, after_skill.source);
    assert_eq!(before_skill.version, after_skill.version);
    assert_eq!(before_skill.revision, after_skill.revision);
    assert_eq!(before_skill.skills, after_skill.skills);
    assert_eq!(
        after_skill.targets,
        [
            aru::manifest::Target::Codex,
            aru::manifest::Target::Copilot,
            aru::manifest::Target::Opencode,
            aru::manifest::Target::Pi,
        ]
    );
    assert_eq!(before.mcp_servers[0].version, after.mcp_servers[0].version);
    assert_eq!(
        before.mcp_servers[0].metadata_sha256,
        after.mcp_servers[0].metadata_sha256
    );
    assert_eq!(
        after.mcp_servers[0]
            .targets
            .iter()
            .map(|target| target.target)
            .collect::<Vec<_>>(),
        [
            aru::manifest::Target::Codex,
            aru::manifest::Target::Copilot,
            aru::manifest::Target::Opencode,
        ]
    );
    assert!(!project.join(".mcp.json").exists());
    assert!(project.join(".codex/config.toml").is_file());
    assert!(project.join(".github/mcp.json").is_file());
    assert!(project.join("opencode.json").is_file());
    for destination in [
        ".agents/skills/demo",
        ".github/skills/demo",
        ".pi/skills/demo",
        ".opencode/skills/demo",
    ] {
        assert!(project.join(destination).exists(), "missing {destination}");
    }
    assert!(!project.join(".claude/skills/demo").exists());
}
