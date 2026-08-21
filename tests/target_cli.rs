use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn init(project: &Path, targets: &[&str]) {
    let mut command = aru(project);
    command.arg("init");
    for target in targets {
        command.args(["--target", target]);
    }
    command.assert().success();
}

fn create_skill_repository(repository: &Path) {
    std::fs::create_dir(repository).unwrap();
    Command::new("git")
        .current_dir(repository)
        .args(["init", "--quiet"])
        .status()
        .unwrap();
    for (key, value) in [
        ("user.email", "target-tests@example.com"),
        ("user.name", "target tests"),
        ("commit.gpgsign", "false"),
    ] {
        Command::new("git")
            .current_dir(repository)
            .args(["config", key, value])
            .status()
            .unwrap();
    }
    let skill = repository.join("skills/demo");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Target projection test\n---\n# Demo\n",
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

fn add_demo_skill(project: &Path, repository: &Path) {
    aru(project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();
}

#[test]
fn target_help_and_list_expose_the_persistent_command_contract() {
    cargo_bin_cmd!("aru")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("target"));
    cargo_bin_cmd!("aru")
        .args(["target", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("remove"))
        .stdout(predicate::str::contains("set"))
        .stdout(predicate::str::contains("list"));

    let temporary = tempfile::tempdir().unwrap();
    init(temporary.path(), &["claude", "agents", "codex"]);
    aru(temporary.path())
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("agents\ncodex\nclaude\n");
}

#[test]
fn available_targets_and_aliases_are_deterministic_and_canonical() {
    cargo_bin_cmd!("aru")
        .args(["target", "list", "--available"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with(
            "adal\t.adal/skills\tskills\t-\n",
        ))
        .stdout(predicate::str::contains(
            "claude\t.claude/skills\tinstructions,skills,mcp\tclaude-code\n",
        ))
        .stdout(predicate::str::contains(
            "kiro\t.kiro/skills\tskills\tkiro-cli\n",
        ));

    let temporary = tempfile::tempdir().unwrap();
    init(temporary.path(), &["claude-code", "kiro-cli"]);
    aru(temporary.path())
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("claude\nkiro\n");
    let manifest = std::fs::read_to_string(temporary.path().join("aru.toml")).unwrap();
    assert!(manifest.contains("targets = [\"claude\", \"kiro\"]"));
    assert!(!manifest.contains("claude-code"));
    assert!(!manifest.contains("kiro-cli"));
}

#[test]
fn skill_only_targets_project_skills_and_reject_unsupported_resources() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["kiro-cli"]);

    add_demo_skill(&project, &repository);
    assert!(project.join(".kiro/skills/demo").is_dir());
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.skill_packages[0].targets,
        [aru::manifest::Target::Kiro]
    );
    aru(&project)
        .args(["metadata", "--format-version", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"kiro\""));
    aru(&project)
        .args(["export", "--format", "cyclonedx1.5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("kiro"));

    std::fs::write(project.join("AGENTS.md"), "# Instructions\n").unwrap();
    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();
    let lock_before = std::fs::read(project.join("aru.lock")).unwrap();
    aru(&project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no configured target that supports instructions",
        ));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read(project.join("aru.lock")).unwrap(),
        lock_before
    );

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
            "no configured target supports MCP",
        ));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read(project.join("aru.lock")).unwrap(),
        lock_before
    );
}

#[test]
fn explicit_skill_path_exceptions_project_to_registered_destinations() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["openclaw", "posit-assistant", "droid"]);
    add_demo_skill(&project, &repository);

    for destination in [
        "skills/demo/SKILL.md",
        ".posit/assistant/skills/demo/SKILL.md",
        ".factory/skills/demo/SKILL.md",
    ] {
        assert!(project.join(destination).is_file(), "missing {destination}");
    }
}

#[test]
fn mixed_targets_filter_implicit_instruction_and_mcp_reach() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    init(project, &["claude", "kiro"]);
    std::fs::write(project.join("AGENTS.md"), "# Instructions\n").unwrap();

    aru(project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();
    aru(project)
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

    let lock = aru::lockfile::Lockfile::load_optional(project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.instruction_sources[0].targets,
        [aru::manifest::Target::Claude]
    );
    assert_eq!(lock.mcp_servers[0].targets.len(), 1);
    assert_eq!(
        lock.mcp_servers[0].targets[0].target,
        aru::manifest::Target::Claude
    );
}

#[test]
fn shared_skill_only_targets_coalesce_and_transition_safely() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["amp", "cursor", "kiro"]);
    add_demo_skill(&project, &repository);

    assert!(project.join(".agents/skills/demo").is_dir());
    #[cfg(unix)]
    assert!(
        std::fs::symlink_metadata(project.join(".kiro/skills/demo"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.skill_packages[0].targets,
        [
            aru::manifest::Target::Amp,
            aru::manifest::Target::Cursor,
            aru::manifest::Target::Kiro,
        ]
    );
    let state: aru::ownership::State =
        toml::from_str(&std::fs::read_to_string(project.join(".aru/state.toml")).unwrap()).unwrap();
    assert_eq!(
        state
            .entries
            .iter()
            .filter(|entry| entry.kind == "skill" && entry.key == "demo")
            .count(),
        2
    );

    aru(&project)
        .args(["target", "remove", "amp"])
        .assert()
        .success();
    assert!(project.join(".agents/skills/demo").is_dir());

    aru(&project)
        .args(["target", "remove", "cursor"])
        .assert()
        .success();
    assert!(!project.join(".agents/skills/demo").exists());
    assert!(project.join(".kiro/skills/demo").is_dir());
    assert!(
        !std::fs::symlink_metadata(project.join(".kiro/skills/demo"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn target_add_remove_and_set_apply_exact_persistent_sets() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    init(project, &["codex"]);

    aru(project)
        .args(["target", "add", "claude"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Added target claude"))
        .stderr(predicate::str::contains(
            "Targets synchronized: codex, claude.",
        ));
    assert!(
        std::fs::read_to_string(project.join("aru.toml"))
            .unwrap()
            .contains("targets = [\"codex\", \"claude\"]")
    );

    aru(project)
        .args(["target", "add", "claude"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Project is synchronized."));

    aru(project)
        .args(["target", "remove", "claude"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Removed target claude"))
        .stderr(predicate::str::contains("Targets synchronized: codex."));

    let before = std::fs::read(project.join("aru.toml")).unwrap();
    aru(project)
        .args(["target", "remove", "codex"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot remove the last target"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);

    aru(project)
        .args(["target", "set", "claude"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Added target claude"))
        .stderr(predicate::str::contains("Removed target codex"))
        .stderr(predicate::str::contains("Targets synchronized: claude."));
    aru(project)
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("claude\n");

    aru(project)
        .args(["target", "remove", "codex"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "target \"codex\" is not configured",
        ));
}

#[test]
fn agents_target_projects_shared_skills_and_rejects_mcp_without_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["agents", "codex"]);

    let assert = aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert_eq!(stderr.matches("Created skill demo").count(), 1);
    assert!(project.join(".agents/skills/demo").is_dir());

    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.skill_packages[0].targets,
        [aru::manifest::Target::Agents, aru::manifest::Target::Codex,]
    );
    let state: aru::ownership::State =
        toml::from_str(&std::fs::read_to_string(project.join(".aru/state.toml")).unwrap()).unwrap();
    assert_eq!(
        state
            .entries
            .iter()
            .filter(|entry| entry.kind == "skill" && entry.key == "demo")
            .count(),
        1
    );

    aru(&project)
        .args(["target", "set", "agents"])
        .assert()
        .success();
    assert!(project.join(".agents/skills/demo").is_dir());
    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();
    let lock_before = std::fs::read(project.join("aru.lock")).unwrap();
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
            "no configured target supports MCP",
        ));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read(project.join("aru.lock")).unwrap(),
        lock_before
    );
}

#[test]
fn target_dry_run_and_no_sync_make_pending_projection_state_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    init(project, &["codex"]);
    aru(project)
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
    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();
    let lock_before = std::fs::read(project.join("aru.lock")).unwrap();
    let state_before = std::fs::read(project.join(".aru/state.toml")).unwrap();

    aru(project)
        .args(["target", "add", "claude", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would add target claude"))
        .stderr(predicate::str::contains(
            "Would create MCP docs (.mcp.json)",
        ));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read(project.join("aru.lock")).unwrap(),
        lock_before
    );
    assert_eq!(
        std::fs::read(project.join(".aru/state.toml")).unwrap(),
        state_before
    );
    assert!(!project.join(".mcp.json").exists());

    let locked_before = aru::lockfile::Lockfile::load_optional(project)
        .unwrap()
        .unwrap();
    aru(project)
        .args(["target", "add", "claude", "--no-sync"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Target paths were not changed; run `aru sync` to apply.",
        ));
    let locked_after = aru::lockfile::Lockfile::load_optional(project)
        .unwrap()
        .unwrap();
    assert_eq!(
        locked_before.mcp_servers[0].version,
        locked_after.mcp_servers[0].version
    );
    assert_eq!(
        locked_before.mcp_servers[0].metadata_sha256,
        locked_after.mcp_servers[0].metadata_sha256
    );
    assert_eq!(locked_after.mcp_servers[0].targets.len(), 2);
    assert!(!project.join(".mcp.json").exists());
    assert_eq!(
        std::fs::read(project.join(".aru/state.toml")).unwrap(),
        state_before
    );

    aru(project).arg("sync").assert().success();
    assert!(project.join(".mcp.json").is_file());
}

#[test]
fn all_targets_project_skills_to_native_paths_and_replay_locked() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(
        &project,
        &["agents", "codex", "claude", "copilot", "pi", "opencode"],
    );

    add_demo_skill(&project, &repository);

    for destination in [
        ".agents/skills/demo",
        ".claude/skills/demo",
        ".github/skills/demo",
        ".pi/skills/demo",
        ".opencode/skills/demo",
    ] {
        assert!(project.join(destination).exists(), "missing {destination}");
    }
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.skill_packages[0].targets,
        [
            aru::manifest::Target::Agents,
            aru::manifest::Target::Codex,
            aru::manifest::Target::Claude,
            aru::manifest::Target::Copilot,
            aru::manifest::Target::Opencode,
            aru::manifest::Target::Pi,
        ]
    );

    for directory in [".agents", ".claude", ".github", ".pi", ".opencode"] {
        std::fs::remove_dir_all(project.join(directory)).unwrap();
    }
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(&project).args(["sync", "--locked"]).assert().success();
    for destination in [
        ".agents/skills/demo",
        ".claude/skills/demo",
        ".github/skills/demo",
        ".pi/skills/demo",
        ".opencode/skills/demo",
    ] {
        assert!(project.join(destination).exists(), "missing {destination}");
    }
}

#[test]
fn target_skill_projection_matches_the_exact_set_across_layout_transitions() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["claude"]);
    add_demo_skill(&project, &repository);
    let initial_lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();

    assert!(!project.join(".agents/skills/demo").exists());
    assert!(project.join(".claude/skills/demo").is_dir());
    assert!(
        !std::fs::symlink_metadata(project.join(".claude/skills/demo"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    aru(&project)
        .args(["target", "set", "codex", "claude"])
        .assert()
        .success();
    assert!(project.join(".agents/skills/demo").is_dir());
    assert!(project.join(".claude/skills/demo").exists());
    #[cfg(unix)]
    assert!(
        std::fs::symlink_metadata(project.join(".claude/skills/demo"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    aru(&project)
        .args(["target", "set", "claude"])
        .assert()
        .success();
    assert!(!project.join(".agents/skills/demo").exists());
    assert!(project.join(".claude/skills/demo").is_dir());
    assert!(
        !std::fs::symlink_metadata(project.join(".claude/skills/demo"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    aru(&project)
        .args(["target", "set", "codex"])
        .assert()
        .success();
    assert!(project.join(".agents/skills/demo").is_dir());
    assert!(!project.join(".claude/skills/demo").exists());
    let final_lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    let initial = &initial_lock.skill_packages[0];
    let final_package = &final_lock.skill_packages[0];
    assert_eq!(initial.source, final_package.source);
    assert_eq!(initial.requirement, final_package.requirement);
    assert_eq!(initial.version, final_package.version);
    assert_eq!(initial.revision, final_package.revision);
    assert_eq!(initial.skills, final_package.skills);
    assert_eq!(
        initial_lock.package_input_hash,
        final_lock.package_input_hash
    );
    assert_eq!(initial.targets, [aru::manifest::Target::Claude]);
    assert_eq!(final_package.targets, [aru::manifest::Target::Codex]);
}

#[test]
fn target_add_rejects_unmanaged_collision_unless_force_is_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["codex"]);
    add_demo_skill(&project, &repository);
    let collision = project.join(".claude/skills/demo");
    std::fs::create_dir_all(&collision).unwrap();
    std::fs::write(
        collision.join("SKILL.md"),
        "---\nname: demo\ndescription: Unmanaged\n---\n# Manual\n",
    )
    .unwrap();
    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["target", "add", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert!(
        std::fs::read_to_string(collision.join("SKILL.md"))
            .unwrap()
            .contains("Unmanaged")
    );

    aru(&project)
        .args(["target", "add", "claude", "--force", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Would force replace skill demo (.claude/skills/demo)",
        ));
    aru(&project)
        .args(["target", "add", "claude", "--force"])
        .assert()
        .success();
    #[cfg(unix)]
    assert!(
        std::fs::symlink_metadata(&collision)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn target_remove_rejects_drift_without_changing_the_declared_set() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["codex", "claude"]);
    add_demo_skill(&project, &repository);
    let claude_skill = project.join(".claude/skills/demo");
    std::fs::remove_file(&claude_skill).unwrap();
    std::fs::create_dir(&claude_skill).unwrap();
    std::fs::write(
        claude_skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Drifted\n---\n# Manual\n",
    )
    .unwrap();
    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["target", "remove", "claude"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("drift"));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert!(
        std::fs::read_to_string(claude_skill.join("SKILL.md"))
            .unwrap()
            .contains("Drifted")
    );
}

#[cfg(unix)]
#[test]
fn target_set_with_missing_state_preserves_removed_artifacts_and_reports_them() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_skill_repository(&repository);
    init(&project, &["codex", "claude"]);
    add_demo_skill(&project, &repository);
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
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    let manifest_before = std::fs::read(project.join("aru.toml")).unwrap();
    let lock_before = std::fs::read(project.join("aru.lock")).unwrap();

    aru(&project)
        .args(["target", "set", "claude", "--no-sync"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot defer the target change with missing local ownership state",
        ));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        manifest_before
    );
    assert_eq!(
        std::fs::read(project.join("aru.lock")).unwrap(),
        lock_before
    );

    aru(&project)
        .args(["target", "set", "claude"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Warning"))
        .stderr(predicate::str::contains(".agents/skills/demo"))
        .stderr(predicate::str::contains(".codex/config.toml"));

    assert!(project.join(".agents/skills/demo").is_dir());
    assert!(project.join(".codex/config.toml").is_file());
    assert!(project.join(".claude/skills/demo").is_dir());
    assert!(
        !std::fs::symlink_metadata(project.join(".claude/skills/demo"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    aru(&project)
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("claude\n");
}

#[test]
fn target_no_sync_warns_before_forgetting_unowned_removed_projections() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    init(project, &["codex", "claude"]);
    aru(project)
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
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();

    aru(project)
        .args(["target", "set", "codex", "--no-sync"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Warning"))
        .stderr(predicate::str::contains(".mcp.json"))
        .stderr(predicate::str::contains("Target paths were not changed"));
    assert!(project.join(".mcp.json").is_file());
}

#[test]
fn target_remove_preserves_unrelated_mcp_configuration() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    init(project, &["codex"]);
    std::fs::write(
        project.join(".mcp.json"),
        r#"{"custom":{"keep":true},"mcpServers":{"unmanaged":{"command":"keep"}}}"#,
    )
    .unwrap();
    aru(project)
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
    aru(project)
        .args(["target", "add", "claude"])
        .assert()
        .success();
    aru(project)
        .args(["target", "remove", "claude"])
        .assert()
        .success();

    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(config["custom"]["keep"], true);
    assert_eq!(config["mcpServers"]["unmanaged"]["command"], "keep");
    assert!(config["mcpServers"].get("docs").is_none());
}
