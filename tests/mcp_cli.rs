use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn init_targets(project: &Path, targets: &[&str]) {
    std::fs::create_dir(project).unwrap();
    let mut command = aru(project);
    command.arg("init");
    for target in targets {
        command.args(["--target", target]);
    }
    command.assert().success();
}

fn init(project: &Path) {
    init_targets(project, &["codex", "claude"]);
}

#[test]
fn mcp_add_help_groups_sources_and_describes_apply_options() {
    cargo_bin_cmd!("aru")
        .args(["mcp", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Source Options:"))
        .stdout(predicate::str::contains("Registry Options:"))
        .stdout(predicate::str::contains("Apply Options:"))
        .stdout(predicate::str::contains(
            "Environment variable containing a bearer token",
        ))
        .stdout(predicate::str::contains("--env-var <NAME>"))
        .stdout(predicate::str::contains("--header-env <HEADER=ENV>"))
        .stdout(predicate::str::contains(
            "npm or PyPI with an explicit uvx hint",
        ))
        .stdout(predicate::str::contains("Codex-only").not())
        .stdout(predicate::str::contains(
            "Update manifest and lock but skip target project paths",
        ));
}

#[test]
fn mcp_list_does_not_invent_an_unresolved_registry_transport() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::write(
        project.join("aru.toml"),
        "[project]\ntargets = [\"codex\"]\n\n[mcp.docs]\nregistry = \"https://registry.example\"\nserver = \"io.example/docs\"\n",
    )
    .unwrap();

    aru(project)
        .args(["mcp", "list"])
        .assert()
        .success()
        .stdout("docs\tregistry\tunresolved\n");
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
        .args(["mcp", "list"])
        .assert()
        .success()
        .stdout("yfinance\tstdio\tstdio\n");

    aru(&project)
        .args(["sync", "--locked"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Project is synchronized."));
}

#[test]
fn direct_environment_references_are_projected_without_reading_secret_values() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init_targets(&project, &["codex", "claude", "copilot", "opencode"]);

    aru(&project)
        .env("GITHUB_TOKEN", "marker-secret-must-not-be-persisted")
        .args([
            "mcp",
            "add",
            "--command",
            "docker",
            "--arg",
            "github-mcp",
            "--env-var",
            "GITHUB_TOKEN",
            "--name",
            "github",
        ])
        .assert()
        .success();
    aru(&project)
        .env("DOCS_API_KEY", "second-marker-secret-must-not-be-persisted")
        .args([
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--header-env",
            "X-API-Key=DOCS_API_KEY",
            "--name",
            "docs",
        ])
        .assert()
        .success();

    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("env-vars = [\"GITHUB_TOKEN\"]"));
    assert!(manifest.contains("[mcp.docs.env-http-headers]"));
    assert!(manifest.contains("X-API-Key = \"DOCS_API_KEY\""));

    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    let github = lock
        .mcp_servers
        .iter()
        .find(|server| server.name == "github")
        .unwrap();
    assert!(
        github
            .targets
            .iter()
            .all(|target| target.env_vars == ["GITHUB_TOKEN"])
    );
    let docs = lock
        .mcp_servers
        .iter()
        .find(|server| server.name == "docs")
        .unwrap();
    assert!(docs.targets.iter().all(|target| {
        target.env_http_headers.get("X-API-Key").map(String::as_str) == Some("DOCS_API_KEY")
    }));

    let codex = std::fs::read_to_string(project.join(".codex/config.toml")).unwrap();
    assert!(codex.contains("env_vars = [\"GITHUB_TOKEN\"]"));
    assert!(codex.contains("X-API-Key = \"DOCS_API_KEY\""));

    let claude: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(
        claude["mcpServers"]["github"]["env"]["GITHUB_TOKEN"],
        "${GITHUB_TOKEN}"
    );
    assert_eq!(
        claude["mcpServers"]["docs"]["headers"]["X-API-Key"],
        "${DOCS_API_KEY}"
    );

    let copilot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".github/mcp.json")).unwrap()).unwrap();
    assert_eq!(
        copilot["mcpServers"]["github"]["env"]["GITHUB_TOKEN"],
        "${GITHUB_TOKEN}"
    );
    assert_eq!(
        copilot["mcpServers"]["docs"]["headers"]["X-API-Key"],
        "${DOCS_API_KEY}"
    );

    let opencode = std::fs::read_to_string(project.join("opencode.json")).unwrap();
    assert!(opencode.contains("{env:GITHUB_TOKEN}"));
    assert!(opencode.contains("{env:DOCS_API_KEY}"));

    let persisted = [
        "aru.toml",
        "aru.lock",
        ".codex/config.toml",
        ".mcp.json",
        ".github/mcp.json",
        "opencode.json",
    ]
    .iter()
    .map(|path| std::fs::read_to_string(project.join(path)).unwrap())
    .collect::<String>();
    assert!(!persisted.contains("marker-secret-must-not-be-persisted"));
    assert!(!persisted.contains("second-marker-secret-must-not-be-persisted"));

    aru(&project).args(["sync", "--locked"]).assert().success();
    aru(&project).arg("audit").assert().success();
}

#[test]
fn invalid_direct_environment_references_fail_before_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let cases = [
        (
            vec![
                "mcp",
                "add",
                "--command",
                "demo",
                "--env-var",
                "TOKEN",
                "--env-var",
                "TOKEN",
                "--name",
                "demo",
            ],
            "env-vars contains duplicates",
        ),
        (
            vec![
                "mcp",
                "add",
                "--url",
                "https://example.com/mcp",
                "--header-env",
                "missing-assignment",
                "--name",
                "demo",
            ],
            "expected HEADER=ENV",
        ),
        (
            vec![
                "mcp",
                "add",
                "--url",
                "https://example.com/mcp",
                "--header-env",
                "Authorization=OTHER_TOKEN",
                "--bearer-token-env",
                "TOKEN",
                "--name",
                "demo",
            ],
            "cannot combine bearer-token-env",
        ),
    ];

    for (index, (args, expected)) in cases.into_iter().enumerate() {
        let project = temporary.path().join(format!("project-{index}"));
        init_targets(&project, &["codex"]);
        let manifest = std::fs::read(project.join("aru.toml")).unwrap();
        aru(&project)
            .args(args)
            .assert()
            .failure()
            .stderr(predicate::str::contains(expected));
        assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
        assert!(!project.join("aru.lock").exists());
        assert!(!project.join(".codex").exists());
    }
}

#[test]
fn copilot_and_opencode_project_mcp_preserve_user_config_and_replay_locked() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init_targets(&project, &["copilot", "opencode"]);
    std::fs::create_dir(project.join(".github")).unwrap();
    std::fs::write(
        project.join(".github/mcp.json"),
        r#"{"custom":{"keep":true},"mcpServers":{"unmanaged":{"command":"keep"}}}"#,
    )
    .unwrap();
    std::fs::write(
        project.join("opencode.json"),
        r#"{
  // preserve this comment
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "unmanaged": { "type": "local", "command": ["keep"] },
  },
}
"#,
    )
    .unwrap();

    aru(&project)
        .args([
            "mcp",
            "add",
            "--command",
            "uvx",
            "--arg",
            "demo-mcp@1.0.0",
            "--name",
            "demo",
        ])
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
            "--bearer-token-env",
            "DOCS_MCP_TOKEN",
        ])
        .assert()
        .success();

    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    for server in &lock.mcp_servers {
        assert_eq!(
            server
                .targets
                .iter()
                .map(|target| target.target)
                .collect::<Vec<_>>(),
            [
                aru::manifest::Target::Copilot,
                aru::manifest::Target::Opencode,
            ]
        );
    }

    let copilot: serde_json::Value =
        serde_json::from_slice(&std::fs::read(project.join(".github/mcp.json")).unwrap()).unwrap();
    assert_eq!(copilot["custom"]["keep"], true);
    assert_eq!(copilot["mcpServers"]["unmanaged"]["command"], "keep");
    assert_eq!(copilot["mcpServers"]["demo"]["type"], "stdio");
    assert_eq!(copilot["mcpServers"]["demo"]["command"], "uvx");
    assert_eq!(
        copilot["mcpServers"]["demo"]["args"],
        serde_json::json!(["demo-mcp@1.0.0"])
    );
    assert_eq!(
        copilot["mcpServers"]["demo"]["tools"],
        serde_json::json!(["*"])
    );
    assert_eq!(copilot["mcpServers"]["docs"]["type"], "http");
    assert_eq!(
        copilot["mcpServers"]["docs"]["headers"]["Authorization"],
        "Bearer ${DOCS_MCP_TOKEN}"
    );

    let opencode = std::fs::read_to_string(project.join("opencode.json")).unwrap();
    assert!(opencode.contains("// preserve this comment"));
    assert!(opencode.contains("\"unmanaged\""));
    assert!(opencode.contains("\"type\": \"local\""));
    assert!(opencode.contains("\"command\": ["));
    assert!(opencode.contains("\"uvx\""));
    assert!(opencode.contains("\"demo-mcp@1.0.0\""));
    assert!(opencode.contains("\"type\": \"remote\""));
    assert!(opencode.contains("Bearer {env:DOCS_MCP_TOKEN}"));
    assert!(opencode.contains("\"oauth\": false"));

    std::fs::remove_file(project.join(".github/mcp.json")).unwrap();
    std::fs::remove_file(project.join("opencode.json")).unwrap();
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(&project).args(["sync", "--locked"]).assert().success();
    assert!(project.join(".github/mcp.json").is_file());
    assert!(project.join("opencode.json").is_file());
    aru(&project).arg("audit").assert().success();
}

#[test]
fn pi_rejects_mcp_before_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init_targets(&project, &["pi"]);
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

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

    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".pi/mcp.json").exists());
}

#[test]
fn invalid_opencode_config_fails_before_project_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init_targets(&project, &["opencode"]);
    std::fs::write(project.join("opencode.json"), "{ invalid").unwrap();
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["mcp", "add", "--command", "uvx", "--name", "demo"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid JSONC in opencode.json"));

    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert_eq!(
        std::fs::read(project.join("opencode.json")).unwrap(),
        b"{ invalid"
    );
    assert!(!project.join("aru.lock").exists());
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
