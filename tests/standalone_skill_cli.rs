use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn git(repository: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success(), "git {arguments:?} failed");
}

fn create_repository(repository: &Path, skills: &[&str]) {
    std::fs::create_dir(repository).unwrap();
    git(repository, &["init", "--quiet"]);
    git(
        repository,
        &["config", "user.email", "standalone@example.com"],
    );
    git(repository, &["config", "user.name", "standalone tests"]);
    git(repository, &["config", "commit.gpgsign", "false"]);
    for name in skills {
        let directory = repository.join("skills").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Standalone {name}\n---\n# {name}\n"),
        )
        .unwrap();
    }
    git(repository, &["add", "skills"]);
    git(repository, &["commit", "--quiet", "-m", "initial"]);
    git(repository, &["tag", "1.0.0"]);
}

#[test]
fn explicit_target_installs_without_project_state() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["demo"]);

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .args([
            "skill",
            "add",
            "--target",
            "codex",
            repository.to_str().unwrap(),
            "--all",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Standalone skills installed"));

    assert!(project.join(".agents/skills/demo/SKILL.md").is_file());
    assert!(!project.join("aru.toml").exists());
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru").exists());
}

#[test]
fn global_flags_install_to_target_user_directories_without_project_state() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let codex_home = temporary.path().join("codex-home");
    let config_home = temporary.path().join("config-home");
    let state_home = temporary.path().join("state-home");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&home).unwrap();
    create_repository(&repository, &["alpha", "beta"]);

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("CODEX_HOME", &codex_home)
        .env("XDG_CONFIG_HOME", &config_home)
        .env("XDG_STATE_HOME", &state_home)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "alpha",
            "-g",
            "--target",
            "codex",
            "--target",
            "opencode",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Global skills installed"));

    assert!(codex_home.join("skills/alpha/SKILL.md").is_file());
    assert!(config_home.join("opencode/skills/alpha/SKILL.md").is_file());
    assert!(!project.join(".agents").exists());
    assert!(!project.join("aru.toml").exists());
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru").exists());
    assert!(!state_home.exists());

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env_remove("CODEX_HOME")
        .env("XDG_CONFIG_HOME", "relative-config")
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--skill",
            "beta",
            "--global",
            "--target",
            "pi",
            "--dry-run",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would create skill beta"));
    assert!(!home.join(".pi/agent/skills/beta").exists());
}

#[test]
fn distinct_global_target_aliases_preserve_their_requested_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let config_home = temporary.path().join("config-home");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&home).unwrap();
    create_repository(&repository, &["demo"]);

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("XDG_CONFIG_HOME", &config_home)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--global",
            "--target",
            "agents",
            "--target",
            "universal",
            "--target",
            "antigravity",
            "--target",
            "antigravity-cli",
            "--target",
            "qoder",
            "--target",
            "qoder-cn",
            "--target",
            "trae",
            "--target",
            "trae-cn",
        ])
        .assert()
        .success();

    assert!(config_home.join("agents/skills/demo/SKILL.md").is_file());
    assert!(
        home.join(".gemini/antigravity-cli/skills/demo/SKILL.md")
            .is_file()
    );
    assert!(home.join(".qoder-cn/skills/demo/SKILL.md").is_file());
    assert!(home.join(".trae-cn/skills/demo/SKILL.md").is_file());
    assert!(home.join(".agents/skills/demo/SKILL.md").is_file());
    assert!(
        home.join(".gemini/antigravity/skills/demo/SKILL.md")
            .is_file()
    );
    assert!(home.join(".qoder/skills/demo/SKILL.md").is_file());
    assert!(home.join(".trae/skills/demo/SKILL.md").is_file());
}

#[test]
fn distinct_global_targets_share_one_install_per_destination() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let config = temporary.path().join("config");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&home).unwrap();
    create_repository(&repository, &["demo"]);

    for dry_run in [true, false] {
        let mut command = cargo_bin_cmd!("aru");
        command
            .current_dir(&project)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("XDG_CONFIG_HOME", &config)
            .args([
                "skill",
                "add",
                repository.to_str().unwrap(),
                "--all",
                "--global",
                "--target",
                "agents",
                "--target",
                "cline",
                "--target",
                "amp",
                "--target",
                "replit",
            ]);
        if dry_run {
            command.arg("--dry-run");
        }
        let result = command.assert().success();
        let stderr = String::from_utf8_lossy(&result.get_output().stderr);
        let action = if dry_run { "Would create" } else { "Created" };
        assert_eq!(stderr.matches(&format!("{action} skill demo")).count(), 2);
        if dry_run {
            assert!(stderr.contains("no project or target files were changed"));
            assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
        }
        assert_eq!(
            home.join(".agents/skills/demo/SKILL.md").is_file(),
            !dry_run
        );
        assert_eq!(
            config.join("agents/skills/demo/SKILL.md").is_file(),
            !dry_run
        );
    }
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
}

#[test]
fn preview_help_discloses_private_user_lock_metadata() {
    for command in [
        vec!["skill", "add", "--help"],
        vec!["mcp", "add", "--help"],
        vec!["sync", "--help"],
        vec!["lock", "--help"],
    ] {
        cargo_bin_cmd!("aru")
            .args(command)
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "may create private user lock metadata",
            ));
    }
}

#[test]
fn repeated_global_targets_and_equivalent_aliases_are_rejected() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&home).unwrap();
    create_repository(&repository, &["demo"]);
    for targets in [["agents", "agents"], ["claude", "claude-code"]] {
        cargo_bin_cmd!("aru")
            .current_dir(&project)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env_remove("CLAUDE_CONFIG_DIR")
            .args([
                "skill",
                "add",
                repository.to_str().unwrap(),
                "--all",
                "--global",
                "--target",
                targets[0],
                "--target",
                targets[1],
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains("targets contains duplicates"));
    }
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
}

#[cfg(windows)]
#[test]
fn windows_global_install_uses_profile_despite_invalid_home() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let profile = temporary.path().join("profile");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&profile).unwrap();
    create_repository(&repository, &["demo"]);
    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .env("USERPROFILE", &profile)
        .env("HOME", "relative-invalid-home")
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--global",
            "--target",
            "pi",
        ])
        .assert()
        .success();
    assert!(profile.join(".pi/agent/skills/demo/SKILL.md").is_file());
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
}

#[test]
fn complete_target_override_does_not_require_a_home_directory() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let codex_home = temporary.path().join("codex-home");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["demo"]);

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env("CODEX_HOME", &codex_home)
        .env("XDG_CONFIG_HOME", "unrelated-relative-config")
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--global",
            "--target",
            "codex",
        ])
        .assert()
        .success();

    assert!(codex_home.join("skills/demo/SKILL.md").is_file());
    assert!(!project.join(".aru").exists());
}

#[test]
fn global_dry_run_rejects_nested_destinations_before_writing() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    let nested_codex_home = home.join(".claude/skills/demo");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&home).unwrap();
    create_repository(&repository, &["demo"]);

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("CODEX_HOME", &nested_codex_home)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--global",
            "--target",
            "claude",
            "--target",
            "codex",
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "transaction destinations must not be nested",
        ));

    assert!(!home.join(".claude").exists());
}

#[test]
fn case_ambiguous_global_overrides_fail_without_target_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let targets = temporary.path().join("targets");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["demo"]);

    for dry_run in [true, false] {
        let mut command = cargo_bin_cmd!("aru");
        command
            .current_dir(&project)
            .env("CODEX_HOME", targets.join("Root"))
            .env("CLAUDE_CONFIG_DIR", targets.join("root"))
            .args([
                "skill",
                "add",
                repository.to_str().unwrap(),
                "--all",
                "--global",
                "--target",
                "codex",
                "--target",
                "claude",
            ]);
        if dry_run {
            command.arg("--dry-run");
        }
        command
            .assert()
            .failure()
            .stderr(predicate::str::contains("case-ambiguous"));
        assert!(!targets.exists());
        assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
    }
}

#[test]
fn global_collisions_fail_before_any_target_is_written() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let home = temporary.path().join("home");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&home).unwrap();
    create_repository(&repository, &["demo"]);
    let collision = home.join(".claude/skills/demo");
    std::fs::create_dir_all(&collision).unwrap();
    std::fs::write(collision.join("manual"), "keep").unwrap();

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR")
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--global",
            "--target",
            "codex",
            "--target",
            "claude",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));

    assert!(!home.join(".codex/skills/demo").exists());
    assert_eq!(std::fs::read(collision.join("manual")).unwrap(), b"keep");
}

#[test]
fn global_install_rejects_unsupported_targets() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let standalone = temporary.path().join("standalone");
    let home = temporary.path().join("home");
    std::fs::create_dir(&standalone).unwrap();
    std::fs::create_dir(&home).unwrap();
    create_repository(&repository, &["demo"]);

    cargo_bin_cmd!("aru")
        .current_dir(&standalone)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--all",
            "--global",
            "--target",
            "eve",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "eve does not support global Agent Skills installation",
        ));
    assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
}

fn snapshot(root: &Path) -> std::collections::BTreeMap<std::path::PathBuf, Option<Vec<u8>>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(|entry| {
            let entry = entry.unwrap();
            let content = entry
                .file_type()
                .is_file()
                .then(|| std::fs::read(entry.path()).unwrap());
            (
                entry.path().strip_prefix(root).unwrap().to_path_buf(),
                content,
            )
        })
        .collect()
}

#[test]
fn global_install_ignores_project_state_and_preserves_relative_source_base() {
    for scenario in ["root", "nested", "explicit", "invalid-state"] {
        let temporary = tempfile::tempdir().unwrap();
        let managed = temporary.path().join("managed");
        let home = temporary.path().join("home");
        let elsewhere = temporary.path().join("elsewhere");
        std::fs::create_dir(&managed).unwrap();
        std::fs::create_dir(&home).unwrap();
        std::fs::create_dir(&elsewhere).unwrap();
        cargo_bin_cmd!("aru")
            .args([
                "--project",
                managed.to_str().unwrap(),
                "init",
                "--target",
                "claude",
            ])
            .assert()
            .success();
        let base = if matches!(scenario, "nested" | "explicit") {
            managed.join("nested")
        } else {
            managed.clone()
        };
        std::fs::create_dir_all(&base).unwrap();
        create_repository(&base.join("repository"), &["demo"]);
        if scenario == "invalid-state" {
            std::fs::write(managed.join("aru.toml"), "not valid TOML [").unwrap();
            std::fs::write(managed.join("aru.lock"), "not a lockfile").unwrap();
            std::fs::create_dir_all(managed.join(".aru")).unwrap();
            std::fs::write(
                managed.join(".aru/transaction.toml"),
                "pending managed recovery",
            )
            .unwrap();
        }
        let before = snapshot(&managed);
        let destination = home.join(".codex/skills/demo/SKILL.md");
        for dry_run in [true, false] {
            let mut command = cargo_bin_cmd!("aru");
            command
                .current_dir(if scenario == "explicit" {
                    &elsewhere
                } else {
                    &base
                })
                .env("HOME", &home)
                .env("USERPROFILE", &home)
                .env_remove("CODEX_HOME")
                .args([
                    "skill",
                    "add",
                    "repository",
                    "--offline",
                    "--all",
                    "-g",
                    "--target",
                    "codex",
                ]);
            if scenario == "explicit" {
                command.args(["--project", base.to_str().unwrap()]);
            }
            if dry_run {
                command.arg("--dry-run");
            }
            command
                .assert()
                .success()
                .stderr(predicate::str::contains(if dry_run {
                    "Would create skill demo"
                } else {
                    "Global skills installed"
                }));
            assert_eq!(destination.is_file(), !dry_run, "{scenario}");
            assert_eq!(snapshot(&managed), before, "{scenario}");
            assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
        }
    }
}

#[test]
fn global_install_in_managed_project_rejects_project_only_options() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    std::fs::write(project.join("aru.toml"), "not parsed").unwrap();
    let before = snapshot(&project);
    for (option, message) in [
        ("--no-sync", "--no-sync is not supported with --global"),
        (
            "--locked",
            "--locked and --frozen are not supported with --global",
        ),
        (
            "--frozen",
            "--locked and --frozen are not supported with --global",
        ),
    ] {
        cargo_bin_cmd!("aru")
            .current_dir(&project)
            .args([
                "skill", "add", "missing", "--all", "-g", "--target", "codex", option,
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(message));
        assert_eq!(snapshot(&project), before);
    }
}

#[test]
fn project_override_alias_and_explicit_skill_use_the_requested_root() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let elsewhere = temporary.path().join("elsewhere");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&elsewhere).unwrap();
    create_repository(&repository, &["alpha", "beta"]);

    cargo_bin_cmd!("aru")
        .current_dir(&elsewhere)
        .args([
            "--project",
            project.to_str().unwrap(),
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--target",
            "kiro-cli",
            "--skill",
            "beta",
            "--version",
            "=1.0.0",
        ])
        .assert()
        .success();

    assert!(!project.join(".kiro/skills/alpha").exists());
    assert!(project.join(".kiro/skills/beta/SKILL.md").is_file());
    assert!(!elsewhere.join(".kiro").exists());
    assert!(!project.join(".aru").exists());
}

#[test]
fn collisions_fail_atomically_and_force_creates_independent_copies() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["demo"]);
    let collision = project.join(".claude/skills/demo");
    std::fs::create_dir_all(&collision).unwrap();
    std::fs::write(collision.join("manual"), "keep").unwrap();

    let arguments = [
        "skill",
        "add",
        repository.to_str().unwrap(),
        "--all",
        "--target",
        "codex",
        "--target",
        "claude",
    ];
    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .args(arguments)
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));

    assert!(!project.join(".agents/skills/demo").exists());
    assert_eq!(std::fs::read(collision.join("manual")).unwrap(), b"keep");

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .args(arguments)
        .arg("--force")
        .assert()
        .success();

    let agents = project.join(".agents/skills/demo");
    let claude = project.join(".claude/skills/demo");
    assert!(agents.join("SKILL.md").is_file());
    assert!(claude.join("SKILL.md").is_file());
    assert!(!claude.join("manual").exists());
    assert!(
        !std::fs::symlink_metadata(agents)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        !std::fs::symlink_metadata(claude)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(!project.join(".aru").exists());
}

#[test]
fn dry_run_and_standalone_only_option_errors_leave_no_files() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["demo"]);

    cargo_bin_cmd!("aru")
        .current_dir(&project)
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
            "Would create skill demo (.agents/skills/demo)",
        ));
    assert!(!project.join(".agents").exists());
    assert!(!project.join(".aru").exists());

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .args([
            "skill",
            "add",
            "owner/missing",
            "--all",
            "--target",
            "codex",
            "--no-sync",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--no-sync requires"));
    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .args([
            "--locked",
            "skill",
            "add",
            "owner/missing",
            "--all",
            "--target",
            "codex",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "require an initialized aru project",
        ));
}

#[test]
fn noninteractive_standalone_add_requires_target_then_skill_selector() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["demo"]);

    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass --target"));
    cargo_bin_cmd!("aru")
        .current_dir(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--target",
            "codex",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pass --all, --skill, or --path"));
    assert!(!project.join(".aru").exists());
}

#[test]
fn nearest_initialized_ancestor_keeps_managed_behavior() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    let nested = project.join("nested");
    std::fs::create_dir(&project).unwrap();
    std::fs::create_dir(&nested).unwrap();
    create_repository(&repository, &["demo"]);

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
    cargo_bin_cmd!("aru")
        .current_dir(&nested)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();

    assert!(project.join("aru.toml").is_file());
    assert!(project.join("aru.lock").is_file());
    assert!(project.join(".aru/state.toml").is_file());
    assert!(project.join(".agents/skills/demo").is_dir());
    assert!(!nested.join(".agents").exists());
}
