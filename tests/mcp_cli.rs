use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn init(project: &Path) {
    std::fs::create_dir(project).unwrap();
    aru(project)
        .args(["init", "--target", "codex", "--target", "claude"])
        .assert()
        .success();
}

#[test]
fn direct_stdio_command_is_locked_projected_and_replayed() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);

    aru(&project)
        .args([
            "mcp",
            "add",
            "--command",
            "uvx",
            "--arg=--with",
            "--arg",
            "mcp<2",
            "--arg",
            "yfmcp@0.12.2",
            "--name",
            "yfinance",
        ])
        .assert()
        .success();

    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("[mcp.yfinance]"));
    assert!(manifest.contains("command = \"uvx\""));
    assert!(manifest.contains("args = [\"--with\", \"mcp<2\", \"yfmcp@0.12.2\"]"));

    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    let server = lock
        .mcp_servers
        .iter()
        .find(|server| server.name == "yfinance")
        .unwrap();
    assert_eq!(server.version, "direct");
    assert_eq!(server.targets.len(), 2);
    assert!(server.targets.iter().all(|target| {
        target.kind == "command"
            && target.transport == "stdio"
            && target.command.as_deref() == Some("uvx")
            && target.args == ["--with", "mcp<2", "yfmcp@0.12.2"]
            && target.package.is_none()
    }));

    let codex = std::fs::read_to_string(project.join(".codex/config.toml")).unwrap();
    assert!(codex.contains("[mcp_servers.yfinance]"));
    assert!(codex.contains("command = \"uvx\""));
    assert!(codex.contains("args = [\"--with\", \"mcp<2\", \"yfmcp@0.12.2\"]"));
    assert!(codex.contains("enabled = true"));

    let claude: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(claude["mcpServers"]["yfinance"]["command"], "uvx");
    assert_eq!(
        claude["mcpServers"]["yfinance"]["args"],
        serde_json::json!(["--with", "mcp<2", "yfmcp@0.12.2"])
    );

    aru(&project)
        .args(["sync", "--locked"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "aru project is already synchronized",
        ));
}

#[test]
fn direct_stdio_rejects_conflicting_fields_without_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args([
            "mcp",
            "add",
            "--command",
            "uvx",
            "--name",
            "invalid",
            "--version",
            "1.0.0",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "direct stdio MCP \"invalid\" cannot set version",
        ));

    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".codex").exists());
    assert!(!project.join(".mcp.json").exists());
}
