use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn standalone(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.current_dir(project);
    command
}

#[test]
fn direct_url_installs_without_project_state() {
    let project = tempfile::tempdir().unwrap();

    standalone(project.path())
        .args([
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--target",
            "codex",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Standalone MCP installed"));

    let config = std::fs::read_to_string(project.path().join(".codex/config.toml")).unwrap();
    assert!(config.contains("[mcp_servers.docs]"));
    assert!(config.contains("url = \"https://example.com/mcp\""));
    assert!(!project.path().join("aru.toml").exists());
    assert!(!project.path().join("aru.lock").exists());
    assert!(!project.path().join(".aru").exists());
}

#[test]
fn project_override_and_target_alias_use_the_requested_root() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let elsewhere = temporary.path().join("elsewhere");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&elsewhere).unwrap();

    cargo_bin_cmd!("aru")
        .current_dir(&elsewhere)
        .args([
            "--project",
            project.to_str().unwrap(),
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--target",
            "claude-code",
        ])
        .assert()
        .success();

    assert!(project.join(".mcp.json").is_file());
    assert!(!elsewhere.join(".mcp.json").exists());
    assert!(!project.join("aru.toml").exists());
    assert!(!project.join(".aru").exists());
}

#[test]
fn multi_target_stdio_merge_preserves_configs_and_never_executes_command() {
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir(project.path().join(".codex")).unwrap();
    std::fs::create_dir(project.path().join(".github")).unwrap();
    std::fs::write(
        project.path().join(".codex/config.toml"),
        "# keep codex\nmodel = \"x\"\n[mcp_servers.unmanaged]\ncommand = \"keep\"\n",
    )
    .unwrap();
    std::fs::write(
        project.path().join(".mcp.json"),
        r#"{"custom":{"keep":true},"mcpServers":{"unmanaged":{"command":"keep"}}}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join(".github/mcp.json"),
        r#"{"custom":{"keep":true},"mcpServers":{"unmanaged":{"command":"keep"}}}"#,
    )
    .unwrap();
    std::fs::write(
        project.path().join("opencode.json"),
        "{\n  // keep opencode\n  \"mcp\": {\n    \"unmanaged\": { \"type\": \"local\", \"command\": [\"keep\"] },\n  },\n}\n",
    )
    .unwrap();
    let marker = project.path().join("command-was-executed");
    let command = format!("touch {}", marker.display());

    standalone(project.path())
        .env("DEMO_TOKEN", "marker-secret-must-not-be-persisted")
        .args([
            "mcp",
            "add",
            "--command",
            &command,
            "--arg",
            "demo@1.0.0",
            "--env-var",
            "DEMO_TOKEN",
            "--name",
            "demo",
            "--target",
            "codex",
            "--target",
            "claude",
            "--target",
            "copilot",
            "--target",
            "opencode",
        ])
        .assert()
        .success();

    assert!(!marker.exists());
    let codex = std::fs::read_to_string(project.path().join(".codex/config.toml")).unwrap();
    assert!(codex.contains("# keep codex"));
    assert!(codex.contains("[mcp_servers.unmanaged]"));
    assert!(codex.contains("[mcp_servers.demo]"));
    assert!(codex.contains("DEMO_TOKEN"));

    for path in [".mcp.json", ".github/mcp.json"] {
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(project.path().join(path)).unwrap()).unwrap();
        assert_eq!(value["custom"]["keep"], true);
        assert_eq!(value["mcpServers"]["unmanaged"]["command"], "keep");
        assert_eq!(
            value["mcpServers"]["demo"]["env"]["DEMO_TOKEN"],
            "${DEMO_TOKEN}"
        );
    }

    let opencode = std::fs::read_to_string(project.path().join("opencode.json")).unwrap();
    assert!(opencode.contains("// keep opencode"));
    assert!(opencode.contains("\"unmanaged\""));
    assert!(opencode.contains("{env:DEMO_TOKEN}"));

    let persisted = [
        ".codex/config.toml",
        ".mcp.json",
        ".github/mcp.json",
        "opencode.json",
    ]
    .iter()
    .map(|path| std::fs::read_to_string(project.path().join(path)).unwrap())
    .collect::<String>();
    assert!(!persisted.contains("marker-secret-must-not-be-persisted"));
    assert!(!project.path().join(".aru").exists());
}

#[test]
fn same_name_collision_is_atomic_and_force_replaces_only_that_entry() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(
        project.path().join(".mcp.json"),
        r#"{"custom":{"keep":true},"mcpServers":{"docs":{"url":"https://old.example/mcp"},"other":{"command":"keep"}}}"#,
    )
    .unwrap();
    let before = std::fs::read(project.path().join(".mcp.json")).unwrap();
    let arguments = [
        "mcp",
        "add",
        "--url",
        "https://new.example/mcp",
        "--name",
        "docs",
        "--target",
        "codex",
        "--target",
        "claude",
    ];

    standalone(project.path())
        .args(arguments)
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));
    assert_eq!(
        std::fs::read(project.path().join(".mcp.json")).unwrap(),
        before
    );
    assert!(!project.path().join(".codex").exists());

    standalone(project.path())
        .args(arguments)
        .arg("--force")
        .assert()
        .success();
    let claude: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.path().join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(claude["custom"]["keep"], true);
    assert_eq!(claude["mcpServers"]["other"]["command"], "keep");
    assert_eq!(
        claude["mcpServers"]["docs"]["url"],
        "https://new.example/mcp"
    );
    assert!(project.path().join(".codex/config.toml").is_file());
}

#[test]
fn malformed_target_config_blocks_the_complete_transaction() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("opencode.json"), "{ invalid").unwrap();

    standalone(project.path())
        .args([
            "mcp",
            "add",
            "--command",
            "uvx",
            "--name",
            "demo",
            "--target",
            "codex",
            "--target",
            "opencode",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid JSONC"));

    assert!(!project.path().join(".codex").exists());
    assert_eq!(
        std::fs::read(project.path().join("opencode.json")).unwrap(),
        b"{ invalid"
    );
}

#[test]
fn dry_run_and_standalone_policy_errors_write_nothing() {
    let project = tempfile::tempdir().unwrap();

    standalone(project.path())
        .args([
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--target",
            "claude",
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Would create MCP docs (.mcp.json)",
        ));
    assert_eq!(std::fs::read_dir(project.path()).unwrap().count(), 0);

    standalone(project.path())
        .args([
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--target",
            "codex",
            "--no-sync",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--no-sync requires"));
    for policy in ["--locked", "--frozen"] {
        standalone(project.path())
            .args([
                policy,
                "mcp",
                "add",
                "--url",
                "https://example.com/mcp",
                "--name",
                "docs",
                "--target",
                "codex",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "require an initialized aru project",
            ));
    }
    assert_eq!(std::fs::read_dir(project.path()).unwrap().count(), 0);

    standalone(project.path())
        .args([
            "--quiet",
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "quiet",
            "--target",
            "codex",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout("")
        .stderr("");
    assert_eq!(std::fs::read_dir(project.path()).unwrap().count(), 0);
}

#[test]
fn target_validation_and_offline_registry_fail_before_writes() {
    let temporary = tempfile::tempdir().unwrap();
    for (index, arguments) in [
        vec![
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--target",
            "pi",
        ],
        vec![
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--target",
            "codex",
            "--target",
            "codex",
        ],
    ]
    .into_iter()
    .enumerate()
    {
        let project = temporary.path().join(format!("project-{index}"));
        std::fs::create_dir(&project).unwrap();
        standalone(&project)
            .args(arguments)
            .assert()
            .failure()
            .stderr(predicate::str::contains("target"));
        assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
    }

    let project = temporary.path().join("offline");
    std::fs::create_dir(&project).unwrap();
    standalone(&project)
        .args([
            "--offline",
            "mcp",
            "add",
            "io.example/docs",
            "--name",
            "docs",
            "--target",
            "codex",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "offline mode cannot resolve MCP Registry server",
        ));
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
}

#[test]
fn noninteractive_target_omission_fails_before_resolution() {
    let project = tempfile::tempdir().unwrap();

    standalone(project.path())
        .args([
            "--offline",
            "mcp",
            "add",
            "io.example/docs",
            "--name",
            "docs",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass --target"))
        .stderr(predicate::str::contains("offline mode").not());
    assert_eq!(std::fs::read_dir(project.path()).unwrap().count(), 0);
}

#[test]
fn nearest_initialized_ancestor_keeps_managed_mcp_behavior() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let nested = project.join("nested");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&nested).unwrap();
    cargo_bin_cmd!("aru")
        .args([
            "--project",
            project.to_str().unwrap(),
            "init",
            "--target",
            "codex",
        ])
        .assert()
        .success();

    standalone(&nested)
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

    assert!(project.join("aru.toml").is_file());
    assert!(project.join("aru.lock").is_file());
    assert!(project.join(".aru/state.toml").is_file());
    assert!(project.join(".codex/config.toml").is_file());
    assert!(!nested.join(".codex").exists());
}
