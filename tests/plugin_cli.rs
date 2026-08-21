use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn init_git(repository: &Path) {
    std::fs::create_dir_all(repository).unwrap();
    git(repository, &["init", "--quiet"]);
    git(repository, &["config", "user.email", "plugins@example.com"]);
    git(repository, &["config", "user.name", "plugin tests"]);
    git(repository, &["config", "commit.gpgsign", "false"]);
}

fn commit(repository: &Path, tag: &str) {
    git(repository, &["add", "."]);
    git(repository, &["commit", "--quiet", "-m", tag]);
    git(repository, &["tag", tag]);
}

fn init_project(project: &Path, targets: &[&str]) {
    std::fs::create_dir(project).unwrap();
    let mut command = aru(project);
    command.arg("init");
    for target in targets {
        command.args(["--target", target]);
    }
    command.assert().success();
}

fn write_skill(root: &Path, name: &str) {
    let directory = root.join("skills").join(name);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Plugin skill\n---\n# {name}\n"),
    )
    .unwrap();
}

fn write_agent_manifest(root: &Path, name: &str, extension: &str) {
    std::fs::write(
        root.join("plugin.json"),
        format!(
            "{{\"$schema\":\"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json\",\"name\":\"{name}\",\"version\":\"1.0.0\"{extension}}}\n"
        ),
    )
    .unwrap();
}

#[test]
fn plugin_help_exposes_nested_lifecycle_and_selection_contract() {
    cargo_bin_cmd!("aru")
        .args(["plugin", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("info"))
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("remove"));
    cargo_bin_cmd!("aru")
        .args(["plugin", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--subdir"))
        .stdout(predicate::str::contains("--component"))
        .stdout(predicate::str::contains("--skill"))
        .stdout(predicate::str::contains("--mcp"))
        .stdout(predicate::str::contains("--trust-mcp"))
        .stdout(predicate::str::contains("--no-sync"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn agent_plugin_lifecycle_replays_offline_and_exports_identity() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("review-tools");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_agent_manifest(&repository, "review-tools", "");
    write_skill(&repository, "review");
    commit(&repository, "1.0.0");
    init_project(&project, &["codex", "claude"]);
    aru(&project)
        .args(["plugin", "info", repository.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("name:         review-tools"));

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--component",
            "skills",
            "--target",
            "codex",
            "--dry-run",
        ])
        .assert()
        .success();
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/cache").exists());
    assert!(
        !std::fs::read_to_string(project.join("aru.toml"))
            .unwrap()
            .contains("review-tools")
    );

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--component",
            "skills",
            "--target",
            "codex",
            "--no-sync",
        ])
        .assert()
        .success();
    assert!(!project.join(".agents/skills/review/SKILL.md").exists());
    aru(&project).arg("sync").assert().success();
    assert!(project.join(".agents/skills/review/SKILL.md").is_file());
    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("[plugins.review-tools]"));
    assert!(manifest.contains("format = \"agent-plugins\""));
    let lock = std::fs::read_to_string(project.join("aru.lock")).unwrap();
    assert!(lock.contains("version = 4"));
    assert!(lock.contains("[[plugin-package]]"));
    assert!(lock.contains("kind = \"plugin\""));

    aru(&project)
        .args(["--locked", "--offline", "sync", "--dry-run"])
        .assert()
        .success();
    aru(&project).args(["audit"]).assert().success();
    aru(&project)
        .args(["metadata", "--format-version", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"format_version\": 1"))
        .stdout(predicate::str::contains("\"plugins\"").not());
    aru(&project)
        .args(["metadata", "--format-version", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"plugins\""))
        .stdout(predicate::str::contains("review-tools"))
        .stdout(predicate::str::contains("\"origin\""));
    aru(&project)
        .args(["export", "--format", "cyclonedx1.5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("plugin-package"))
        .stdout(predicate::str::contains("aru:plugin-format"));

    let cached_manifest = walkdir::WalkDir::new(project.join(".aru/cache"))
        .into_iter()
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.into_path())
        .find(|path| path.ends_with("content/plugin.json") && path.is_file())
        .unwrap();
    std::fs::write(&cached_manifest, "{}\n").unwrap();
    aru(&project)
        .arg("audit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("plugin.cache"));
    aru(&project).arg("sync").assert().success();
    aru(&project).arg("audit").assert().success();

    aru(&project)
        .args(["plugin", "remove", "review-tools"])
        .assert()
        .success();
    assert!(!project.join(".agents/skills/review").exists());
    assert!(
        !std::fs::read_to_string(project.join("aru.toml"))
            .unwrap()
            .contains("review-tools")
    );
}

#[test]
fn whole_intent_blocks_openai_hooks_but_explicit_skill_selection_succeeds() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("openai-plugin");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_agent_manifest(
        &repository,
        "openai-plugin",
        ",\"extensions\":{\"com.openai\":{\"hooks\":{\"sessionStart\":[]}}}",
    );
    write_skill(&repository, "portable");
    std::fs::create_dir_all(repository.join(".codex-plugin")).unwrap();
    std::fs::write(
        repository.join(".codex-plugin/plugin.json"),
        r#"{"apps":{"legacy":{"id":"ignored-because-inline-wins"}}}"#,
    )
    .unwrap();
    commit(&repository, "1.0.0");
    init_project(&project, &["codex"]);

    aru(&project)
        .args(["plugin", "add", repository.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("openai:hooks"));
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/cache").exists());

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "portable",
        ])
        .assert()
        .success();
    let lock = std::fs::read_to_string(project.join("aru.lock")).unwrap();
    assert!(lock.contains("format = \"openai\""));
    assert!(lock.contains("openai:hooks"));
    assert!(!lock.contains("openai:apps"));
}

#[test]
fn gemini_named_safe_mcp_requires_trust_and_unsafe_wildcard_fails() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("gemini-tools");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_skill(&repository, "gemini-review");
    std::fs::write(
        repository.join("gemini-extension.json"),
        r#"{
  "name": "gemini-tools",
  "version": "1.0.0",
  "commands": [{"name": "native"}],
  "mcpServers": {
    "docs": {"type": "http", "url": "https://example.com/mcp"},
    "bundled": {"type": "stdio", "command": "./bin/server"}
  }
}
"#,
    )
    .unwrap();
    commit(&repository, "1.0.0");
    init_project(&project, &["codex"]);

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--mcp",
            "docs",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("untrusted plugin MCP"));
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/cache").exists());

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--component",
            "mcp",
            "--trust-mcp",
            "docs",
            "--trust-mcp",
            "bundled",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsafe entries"));

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "gemini-review",
            "--mcp",
            "docs",
            "--trust-mcp",
            "docs",
        ])
        .assert()
        .success();
    assert!(
        project
            .join(".agents/skills/gemini-review/SKILL.md")
            .is_file()
    );
    assert!(
        std::fs::read_to_string(project.join(".codex/config.toml"))
            .unwrap()
            .contains("https://example.com/mcp")
    );
}

#[test]
fn plugin_collision_fails_before_intent_writes_and_force_is_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("collision-plugin");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_agent_manifest(&repository, "collision-plugin", "");
    write_skill(&repository, "collision");
    commit(&repository, "1.0.0");
    init_project(&project, &["codex"]);
    let destination = project.join(".agents/skills/collision");
    std::fs::create_dir_all(&destination).unwrap();
    std::fs::write(destination.join("SKILL.md"), "unmanaged\n").unwrap();
    let before = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--component",
            "skills",
        ])
        .assert()
        .failure();
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
    assert!(!project.join("aru.lock").exists());
    assert_eq!(
        std::fs::read_to_string(destination.join("SKILL.md")).unwrap(),
        "unmanaged\n"
    );

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--component",
            "skills",
            "--force",
        ])
        .assert()
        .success();
    assert!(
        std::fs::read_to_string(destination.join("SKILL.md"))
            .unwrap()
            .contains("name: collision")
    );
}

#[test]
fn plugin_update_precise_and_v3_migration_are_controlled() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("updatable-plugin");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_agent_manifest(&repository, "updatable-plugin", "");
    write_skill(&repository, "update-skill");
    commit(&repository, "1.0.0");
    init_project(&project, &["codex"]);

    aru(&project)
        .args([
            "plugin",
            "add",
            repository.to_str().unwrap(),
            "--version",
            "^1.0",
            "--component",
            "skills",
        ])
        .assert()
        .success();
    std::fs::write(repository.join("release"), "1.1.0\n").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "1.1.0"]);
    git(&repository, &["tag", "1.1.0"]);
    aru(&project)
        .args([
            "plugin",
            "update",
            "updatable-plugin",
            "--precise",
            "1.1.0",
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("1.0.0"))
        .stderr(predicate::str::contains("1.1.0"));
    aru(&project)
        .args(["plugin", "update", "updatable-plugin", "--precise", "1.1.0"])
        .assert()
        .success();
    assert!(
        std::fs::read_to_string(project.join("aru.lock"))
            .unwrap()
            .contains("version = \"1.1.0\"")
    );

    let legacy = temporary.path().join("legacy-project");
    init_project(&legacy, &["codex"]);
    std::fs::write(
        legacy.join("aru.lock"),
        "# This file is generated by aru.\nversion = 3\npackage-input-hash = \"sha256:legacy\"\nprojection-input-hash = \"sha256:legacy\"\n",
    )
    .unwrap();
    aru(&legacy)
        .args(["metadata", "--format-version", "1"])
        .assert()
        .success();
    aru(&legacy)
        .args(["lock", "--check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("run `aru lock`"));
    aru(&legacy)
        .args(["--locked", "sync"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires upgrade"));
    aru(&legacy).arg("lock").assert().success();
    assert!(
        std::fs::read_to_string(legacy.join("aru.lock"))
            .unwrap()
            .contains("version = 4")
    );
}

#[test]
fn monorepo_subdirectories_resolve_as_independent_plugins() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("monorepo");
    let project = temporary.path().join("project");
    init_git(&repository);
    for (directory, name, skill) in [
        ("plugins/one", "plugin-one", "one"),
        ("plugins/two", "plugin-two", "two"),
    ] {
        let root = repository.join(directory);
        std::fs::create_dir_all(&root).unwrap();
        write_agent_manifest(&root, name, "");
        write_skill(&root, skill);
    }
    commit(&repository, "1.0.0");
    init_project(&project, &["codex"]);

    for subdir in ["plugins/one", "plugins/two"] {
        aru(&project)
            .args([
                "plugin",
                "add",
                repository.to_str().unwrap(),
                "--subdir",
                subdir,
                "--component",
                "skills",
            ])
            .assert()
            .success();
    }
    aru(&project)
        .args(["plugin", "list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("plugin-one"))
        .stdout(predicate::str::contains("plugin-two"));
    assert!(project.join(".agents/skills/one/SKILL.md").is_file());
    assert!(project.join(".agents/skills/two/SKILL.md").is_file());
}
