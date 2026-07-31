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

fn create_repository(repository: &Path) {
    std::fs::create_dir(repository).unwrap();
    git(repository, &["init", "--quiet"]);
    git(repository, &["config", "user.email", "targets@example.com"]);
    git(repository, &["config", "user.name", "target tests"]);
    git(repository, &["config", "commit.gpgsign", "false"]);
    std::fs::create_dir_all(repository.join("skills/demo")).unwrap();
    std::fs::write(
        repository.join("skills/demo/SKILL.md"),
        "---\nname: demo\ndescription: dependency targets\n---\n# Demo\n",
    )
    .unwrap();
    git(repository, &["add", "."]);
    git(repository, &["commit", "--quiet", "-m", "initial"]);
    git(repository, &["tag", "1.0.0"]);
}

fn init(project: &Path, targets: &[&str]) {
    std::fs::create_dir(project).unwrap();
    let mut command = aru(project);
    command.arg("init");
    for target in targets {
        command.args(["--target", target]);
    }
    command.assert().success();
}

#[test]
fn add_help_exposes_repeatable_dependency_targets() {
    cargo_bin_cmd!("aru")
        .args(["skill", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--target <TARGET>"));
    cargo_bin_cmd!("aru")
        .args(["mcp", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--target <TARGET>"));
}

#[test]
fn skill_and_mcp_dependencies_project_only_to_explicit_targets_and_replay() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    create_repository(&repository);
    init(&project, &["codex", "claude"]);

    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--target",
            "codex",
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
            "--target",
            "claude",
        ])
        .assert()
        .success();

    assert!(project.join(".agents/skills/demo").is_dir());
    assert!(!project.join(".claude/skills/demo").exists());
    assert!(!project.join(".codex/config.toml").exists());
    assert!(project.join(".mcp.json").is_file());

    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("targets = [\"codex\"]"));
    assert!(manifest.contains("targets = [\"claude\"]"));
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(lock.version, 3);
    assert_eq!(
        lock.skill_packages[0].targets,
        [aru::manifest::Target::Codex]
    );
    assert_eq!(
        lock.mcp_servers[0]
            .targets
            .iter()
            .map(|target| target.target)
            .collect::<Vec<_>>(),
        [aru::manifest::Target::Claude]
    );

    std::fs::remove_dir_all(project.join(".agents")).unwrap();
    std::fs::remove_file(project.join(".mcp.json")).unwrap();
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(&project).args(["sync", "--locked"]).assert().success();
    assert!(project.join(".agents/skills/demo").is_dir());
    assert!(!project.join(".claude/skills/demo").exists());
    assert!(!project.join(".codex/config.toml").exists());
    assert!(project.join(".mcp.json").is_file());
    aru(&project).arg("audit").assert().success();
}

#[test]
fn dependency_target_contraction_is_atomic_and_preserves_resolution() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    create_repository(&repository);
    init(&project, &["codex", "claude"]);
    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--target",
            "codex",
            "--target",
            "claude",
        ])
        .assert()
        .success();
    let before = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();

    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--target",
            "codex",
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Would remove skill demo (.claude/skills/demo)",
        ));
    assert!(project.join(".claude/skills/demo").exists());

    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--target",
            "codex",
        ])
        .assert()
        .success();
    assert!(!project.join(".claude/skills/demo").exists());
    let after = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        before.skill_packages[0].version,
        after.skill_packages[0].version
    );
    assert_eq!(
        before.skill_packages[0].revision,
        after.skill_packages[0].revision
    );
    assert_eq!(
        after.skill_packages[0].targets,
        [aru::manifest::Target::Codex]
    );
}

#[test]
fn deferred_target_contraction_keeps_paths_until_sync() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    create_repository(&repository);
    init(&project, &["codex", "claude"]);
    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();

    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--target",
            "codex",
            "--no-sync",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Target paths were not changed"));
    assert!(project.join(".claude/skills/demo").exists());
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.skill_packages[0].targets,
        [aru::manifest::Target::Codex]
    );

    aru(&project).arg("sync").assert().success();
    assert!(!project.join(".claude/skills/demo").exists());
}

#[cfg(unix)]
#[test]
fn dependency_target_contraction_preserves_unowned_paths_after_state_loss() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    create_repository(&repository);
    init(&project, &["codex", "claude"]);
    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();

    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--target",
            "codex",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Warning"))
        .stderr(predicate::str::contains(".claude/skills/demo"));

    assert!(project.join(".claude/skills/demo").exists());
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.skill_packages[0].targets,
        [aru::manifest::Target::Codex]
    );
}

#[test]
fn invalid_dependency_targets_fail_before_fetch_or_write() {
    let temporary = tempfile::tempdir().unwrap();
    for (index, targets) in [vec!["copilot"], vec!["codex", "codex"], vec!["claude"]]
        .into_iter()
        .enumerate()
    {
        let project = temporary.path().join(format!("project-{index}"));
        init(&project, &["codex", "copilot"]);
        let before = std::fs::read(project.join("aru.toml")).unwrap();
        let mut command = aru(&project);
        command.args(["skill", "add", "owner/repository", "--all"]);
        for target in targets {
            command.args(["--target", target]);
        }
        command.assert().failure().stderr(
            predicate::str::contains("dependency target")
                .or(predicate::str::contains("targets contains duplicates")),
        );
        assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
        assert!(!project.join("aru.lock").exists());
        assert!(!project.join(".aru/cache").exists());
    }
}
