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
    git(root, &["config", "commit.gpgsign", "false"]);
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

#[test]
fn init_accepts_a_cargo_style_project_path() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();

    cargo_bin_cmd!("aru")
        .args(["init", project.to_str().unwrap(), "--target", "codex"])
        .assert()
        .success();

    assert!(project.join("aru.toml").is_file());
}

#[test]
fn cargo_style_output_supports_quiet_verbose_and_color_modes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    create_repository(&repository, &["alpha"]);

    let normal = temporary.path().join("normal");
    std::fs::create_dir(&normal).unwrap();
    aru(&normal)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    aru(&normal)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "alpha",
        ])
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains("Locked"))
        .stderr(predicate::str::contains("sha256:").not());

    let verbose = temporary.path().join("verbose");
    std::fs::create_dir(&verbose).unwrap();
    aru(&verbose)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    aru(&verbose)
        .args([
            "--verbose",
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "alpha",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("sha256:"));

    let quiet = temporary.path().join("quiet");
    std::fs::create_dir(&quiet).unwrap();
    aru(&quiet)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    aru(&quiet)
        .args([
            "--quiet",
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "alpha",
        ])
        .assert()
        .success()
        .stdout("")
        .stderr("");

    let color = temporary.path().join("color");
    std::fs::create_dir(&color).unwrap();
    aru(&color)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    aru(&color)
        .args([
            "--color",
            "always",
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
        ])
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains("\u{1b}["));
}

#[test]
fn no_sync_reports_pending_target_paths_for_every_resource_mutation() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--target", "codex"])
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
            "--no-sync",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Target paths were not changed; run `aru sync` to apply.",
        ));
    assert!(!project.join(".codex/config.toml").exists());
}

#[test]
fn failed_apply_does_not_print_successful_plan_actions() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();

    std::fs::write(project.join(".codex"), "blocking file").unwrap();
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
        .stdout("")
        .stderr(predicate::str::contains("Created").not());
    assert!(!project.join("aru.lock").exists());
    assert_eq!(
        std::fs::read_to_string(project.join(".codex")).unwrap(),
        "blocking file"
    );
}

#[test]
fn dry_run_does_not_change_manifest_lock_cache_state_or_projections() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha"]);
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    let before = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would lock skill alpha"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/cache").exists());
    assert!(!project.join(".aru/state.toml").exists());
    assert!(!project.join(".agents").exists());
}

#[test]
fn lock_and_sync_check_are_read_only_and_detect_pending_projection_changes() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();

    aru(&project)
        .args(["lock", "--check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("requires an existing aru.lock"));

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
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();
    let lock = std::fs::read(project.join("aru.lock")).unwrap();
    let state = std::fs::read(project.join(".aru/state.toml")).unwrap();

    aru(&project).args(["lock", "--check"]).assert().success();
    aru(&project).args(["sync", "--check"]).assert().success();
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert_eq!(std::fs::read(project.join("aru.lock")).unwrap(), lock);
    assert_eq!(
        std::fs::read(project.join(".aru/state.toml")).unwrap(),
        state
    );

    std::fs::remove_file(project.join(".codex/config.toml")).unwrap();
    aru(&project)
        .args(["sync", "--check"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("project is not synchronized"));
    assert!(!project.join(".codex/config.toml").exists());
}

#[test]
fn global_locked_and_frozen_reject_commands_that_would_change_the_lock() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

    for policy in ["--locked", "--frozen"] {
        aru(&project)
            .args([
                policy,
                "mcp",
                "add",
                "--url",
                "https://example.com/mcp",
                "--name",
                "docs",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("requires an existing aru.lock"));
        assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
        assert!(!project.join("aru.lock").exists());
        assert!(!project.join(".codex").exists());
    }
}

#[cfg(unix)]
#[test]
fn offline_remote_skill_add_fails_before_invoking_git() {
    use std::os::unix::fs::PermissionsExt;

    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let bin = temporary.path().join("bin");
    let marker = temporary.path().join("git-invoked");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&bin).unwrap();
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();

    let git = bin.join("git");
    std::fs::write(
        &git,
        format!("#!/bin/sh\ntouch '{}'\nexit 99\n", marker.display()),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&git).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&git, permissions).unwrap();

    aru(&project)
        .env("PATH", &bin)
        .args([
            "--offline",
            "skill",
            "add",
            "owner/repository",
            "--rev",
            "0123456789abcdef0123456789abcdef01234567",
            "--skill",
            "demo",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("offline mode"));
    assert!(!marker.exists());
    assert!(!project.join("aru.lock").exists());
}
