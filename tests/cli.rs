use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn git(repository: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success(), "git {arguments:?} failed");
}

fn create_repository(root: &Path, skills: &[&str]) {
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.email", "aru-tests@example.com"]);
    git(root, &["config", "user.name", "aru tests"]);
    for name in skills {
        let directory = root.join("skills").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test {name}\n---\n# {name}\n"),
        )
        .unwrap();
    }
    git(root, &["add", "skills"]);
    git(root, &["commit", "--quiet", "-m", "initial"]);
    git(root, &["tag", "1.0.0"]);
}

fn add_version(repository: &Path, name: &str, tag: &str) {
    std::fs::write(
        repository.join("skills").join(name).join("extra.md"),
        format!("version {tag}\n"),
    )
    .unwrap();
    git(repository, &["add", "."]);
    git(repository, &["commit", "--quiet", "-m", tag]);
    git(repository, &["tag", tag]);
}

#[test]
fn help_exposes_the_v1_command_contract_and_rejects_conflicting_refs() {
    cargo_bin_cmd!("aru")
        .args(["skill", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--skill <NAME>"))
        .stdout(predicate::str::contains("--path <PATH>"))
        .stdout(predicate::str::contains("--no-sync"));

    cargo_bin_cmd!("aru")
        .args([
            "skill",
            "add",
            "owner/repo",
            "--version",
            "1.0.0",
            "--rev",
            "0123456789abcdef",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
    cargo_bin_cmd!("aru")
        .args([
            "skill",
            "add",
            "owner/repo",
            "--skill",
            "review",
            "--path",
            "skills/review",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
    cargo_bin_cmd!("aru")
        .args(["mcp", "add", "--name", "missing-source"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required arguments"));
}

#[test]
fn local_git_add_locked_sync_conservative_update_and_remove() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha"]);

    aru(&project)
        .args(["init", "--agent", "codex", "--agent", "claude-code"])
        .assert()
        .success();
    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "alpha",
            "--version",
            "1.0.0",
        ])
        .assert()
        .success();
    assert!(project.join(".agents/skills/alpha/SKILL.md").is_file());
    #[cfg(unix)]
    assert!(
        std::fs::symlink_metadata(project.join(".claude/skills/alpha"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let expected_skill = std::fs::read(project.join(".agents/skills/alpha/SKILL.md")).unwrap();
    std::fs::remove_dir_all(project.join(".aru/cache")).unwrap();
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    std::fs::remove_dir_all(project.join(".agents")).unwrap();
    std::fs::remove_dir_all(project.join(".claude")).unwrap();
    aru(&project).args(["sync", "--locked"]).assert().success();
    assert_eq!(
        std::fs::read(project.join(".agents/skills/alpha/SKILL.md")).unwrap(),
        expected_skill
    );
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(&project)
        .args(["sync", "--locked"])
        .assert()
        .success()
        .stdout(predicate::str::contains("adopt skill alpha"));

    let first = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(first.skill_packages[0].version, "1.0.0");
    let git_cache = project.join(".aru/cache/git");
    let source_cache = std::fs::read_dir(&git_cache)
        .unwrap()
        .find_map(std::result::Result::ok)
        .unwrap()
        .path();
    let cached_skill = source_cache
        .join(&first.skill_packages[0].revision)
        .join("content/skills/alpha/SKILL.md");
    let mut corrupted = std::fs::read_to_string(&cached_skill).unwrap();
    corrupted.push_str("\nmanual cache corruption\n");
    std::fs::write(&cached_skill, corrupted).unwrap();
    aru(&project).args(["sync", "--locked"]).assert().success();
    assert!(
        !std::fs::read_to_string(&cached_skill)
            .unwrap()
            .contains("manual cache corruption")
    );

    add_version(&repository, "alpha", "1.1.0");
    git(&repository, &["tag", "--force", "1.0.0"]);
    std::fs::remove_dir_all(project.join(".aru/cache")).unwrap();
    aru(&project).args(["sync", "--locked"]).assert().success();
    let replayed = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        replayed.skill_packages[0].revision,
        first.skill_packages[0].revision
    );

    aru(&project).arg("sync").assert().success();
    let conservative = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(conservative.skill_packages[0].version, "1.0.0");

    // Widen the requirement, then explicitly unlock only this package.
    let manifest_path = project.join("aru.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap()
        .replace("version = \"1.0.0\", ", "");
    std::fs::write(&manifest_path, manifest).unwrap();
    aru(&project)
        .args(["skill", "update", repository.to_str().unwrap()])
        .assert()
        .success();
    let updated = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(updated.skill_packages[0].version, "1.1.0");

    aru(&project).args(["sync", "--locked"]).assert().success();
    aru(&project)
        .args([
            "skill",
            "remove",
            repository.to_str().unwrap(),
            "--skill",
            "alpha",
        ])
        .assert()
        .success();
    assert!(!project.join(".agents/skills/alpha").exists());
    assert!(!project.join(".claude/skills/alpha").exists());
}

#[test]
fn named_skill_update_does_not_unlock_other_sources() {
    let temporary = tempfile::tempdir().unwrap();
    let first_repository = temporary.path().join("first");
    let second_repository = temporary.path().join("second");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&first_repository, &["first"]);
    create_repository(&second_repository, &["second"]);
    aru(&project)
        .args(["init", "--agent", "codex"])
        .assert()
        .success();
    for repository in [&first_repository, &second_repository] {
        aru(&project)
            .args(["skill", "add", repository.to_str().unwrap()])
            .assert()
            .success();
    }
    add_version(&first_repository, "first", "1.1.0");
    add_version(&second_repository, "second", "1.1.0");
    aru(&project)
        .args(["skill", "update", first_repository.to_str().unwrap()])
        .assert()
        .success();
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    let versions: std::collections::BTreeMap<_, _> = lock
        .skill_packages
        .iter()
        .map(|package| (package.repository_name.as_str(), package.version.as_str()))
        .collect();
    assert_eq!(versions["first"], "1.1.0");
    assert_eq!(versions["second"], "1.0.0");
}

#[test]
fn named_mcp_update_keeps_untargeted_locked_entry() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--agent", "codex"])
        .assert()
        .success();
    for (name, url) in [
        ("first", "https://first.example.com/mcp"),
        ("second", "https://second.example.com/mcp"),
    ] {
        aru(&project)
            .args(["mcp", "add", "--url", url, "--name", name])
            .assert()
            .success();
    }
    let mut lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    lock.mcp_servers
        .iter_mut()
        .find(|server| server.name == "first")
        .unwrap()
        .version = "selected-sentinel".into();
    lock.mcp_servers
        .iter_mut()
        .find(|server| server.name == "second")
        .unwrap()
        .version = "untargeted-sentinel".into();
    std::fs::write(project.join("aru.lock"), lock.bytes().unwrap()).unwrap();

    aru(&project)
        .args(["mcp", "update", "first"])
        .assert()
        .success();
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    let versions: std::collections::BTreeMap<_, _> = lock
        .mcp_servers
        .iter()
        .map(|server| (server.name.as_str(), server.version.as_str()))
        .collect();
    assert_eq!(versions["first"], "direct");
    assert_eq!(versions["second"], "untargeted-sentinel");
}

#[test]
fn wildcard_exclude_and_explicit_path_intent_are_persisted() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha", "beta"]);
    aru(&project)
        .args(["init", "--agent", "codex"])
        .assert()
        .success();

    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap()])
        .assert()
        .success();
    aru(&project)
        .args([
            "skill",
            "remove",
            repository.to_str().unwrap(),
            "--skill",
            "alpha",
        ])
        .assert()
        .success();
    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("include = [\"*\"]"));
    assert!(manifest.contains("exclude = [\"alpha\"]"));
    assert!(!project.join(".agents/skills/alpha").exists());
    assert!(project.join(".agents/skills/beta").is_dir());

    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--path",
            "skills/alpha",
        ])
        .assert()
        .success();
    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("paths = { alpha = \"skills/alpha\" }"));
    assert!(!manifest.contains("exclude = [\"alpha\"]"));
}

#[test]
fn dry_run_does_not_change_manifest_lock_cache_state_or_projections() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha"]);
    aru(&project)
        .args(["init", "--agent", "codex"])
        .assert()
        .success();
    let before = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dry-run: lock skill alpha"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/cache").exists());
    assert!(!project.join(".aru/state.toml").exists());
    assert!(!project.join(".agents").exists());
}

#[test]
fn locked_sync_rejects_a_missing_lock() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--agent", "codex"])
        .assert()
        .success();
    aru(&project)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires an existing aru.lock"));
}

#[test]
fn agent_change_invalidates_locked_projection_but_normal_sync_reuses_packages() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--agent", "codex"])
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
    let manifest_path = project.join("aru.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap().replace(
        "agents = [\"codex\"]",
        "agents = [\"codex\", \"claude-code\"]",
    );
    std::fs::write(&manifest_path, manifest).unwrap();

    aru(&project)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("projection"));
    aru(&project).arg("sync").assert().success();
    let after = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(before.mcp_servers[0].version, after.mcp_servers[0].version);
    assert_eq!(after.mcp_servers[0].targets.len(), 2);
    assert!(project.join(".mcp.json").is_file());
}

#[test]
fn state_loss_adopts_exact_baseline_and_drift_is_preserved() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--agent", "codex", "--agent", "claude-code"])
        .assert()
        .success();
    aru(&project)
        .env("DOCS_TOKEN", "SUPER_SECRET_VALUE")
        .args([
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--bearer-token-env",
            "DOCS_TOKEN",
        ])
        .assert()
        .success();
    for path in [
        "aru.toml",
        "aru.lock",
        ".aru/state.toml",
        ".codex/config.toml",
        ".mcp.json",
    ] {
        assert!(
            !std::fs::read_to_string(project.join(path))
                .unwrap()
                .contains("SUPER_SECRET_VALUE")
        );
    }
    assert!(!project.join(".aru/transaction.toml").exists());
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(&project)
        .args(["sync", "--locked"])
        .assert()
        .success()
        .stdout(predicate::str::contains("adopt MCP docs"));

    let config_path = project.join(".mcp.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    config["mcpServers"]["docs"]["url"] = "https://manual.example/mcp".into();
    std::fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
    aru(&project)
        .arg("sync")
        .assert()
        .failure()
        .stderr(predicate::str::contains("drift"));
    let preserved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    assert_eq!(
        preserved["mcpServers"]["docs"]["url"],
        "https://manual.example/mcp"
    );
}

#[test]
fn unmanaged_collision_requires_force_and_unknown_orphans_survive_remove() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha"]);
    aru(&project)
        .args(["init", "--agent", "codex"])
        .assert()
        .success();
    let collision = project.join(".agents/skills/alpha");
    std::fs::create_dir_all(&collision).unwrap();
    std::fs::write(
        collision.join("SKILL.md"),
        "---\nname: alpha\ndescription: Unmanaged\n---\n# Manual\n",
    )
    .unwrap();
    let orphan = project.join(".agents/skills/orphan");
    std::fs::create_dir_all(&orphan).unwrap();
    std::fs::write(orphan.join("keep"), "unmanaged").unwrap();

    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));
    assert!(
        std::fs::read_to_string(collision.join("SKILL.md"))
            .unwrap()
            .contains("Unmanaged")
    );
    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--force",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("force replace skill alpha"));
    assert!(
        std::fs::read_to_string(collision.join("SKILL.md"))
            .unwrap()
            .contains("Unmanaged")
    );
    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--force"])
        .assert()
        .success();
    assert!(
        !std::fs::read_to_string(collision.join("SKILL.md"))
            .unwrap()
            .contains("Unmanaged")
    );
    aru(&project)
        .args(["skill", "remove", repository.to_str().unwrap()])
        .assert()
        .success();
    assert!(!collision.exists());
    assert_eq!(std::fs::read(orphan.join("keep")).unwrap(), b"unmanaged");
}
