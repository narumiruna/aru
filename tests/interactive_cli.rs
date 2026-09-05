#![cfg(unix)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use expectrl::{ControlCode, Eof, Expect, Session};

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

fn interactive_add(
    project: &Path,
    repository: &Path,
    extra: &[&str],
) -> expectrl::session::OsSession {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
    command.args([
        "--project",
        project.to_str().unwrap(),
        "skill",
        "add",
        repository.to_str().unwrap(),
    ]);
    command.args(extra);
    let mut session = Session::spawn(command).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(20)));
    session
}

fn standalone_interactive_add(project: &Path, repository: &Path) -> expectrl::session::OsSession {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
    command
        .current_dir(project)
        .args(["skill", "add", repository.to_str().unwrap()]);
    let mut session = Session::spawn(command).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(20)));
    session
}

fn standalone_interactive_mcp_add(project: &Path) -> expectrl::session::OsSession {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
    command.current_dir(project).args([
        "mcp",
        "add",
        "--url",
        "https://example.com/mcp",
        "--name",
        "docs",
    ]);
    let mut session = Session::spawn(command).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(20)));
    session
}

#[test]
fn global_target_selection_ignores_unselected_environment_errors() {
    for target in ["pi", "codex"] {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        let project = temporary.path().join("project");
        let home = temporary.path().join("home");
        let codex = temporary.path().join("codex");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&home).unwrap();
        create_repository(&repository, &["demo"]);
        let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
        command
            .current_dir(&project)
            .args([
                "skill",
                "add",
                repository.to_str().unwrap(),
                "--global",
                "--all",
            ])
            .env("XDG_CONFIG_HOME", "unrelated-relative-config")
            .env("CODEX_HOME", &codex);
        if target == "pi" {
            command.env("HOME", &home);
        } else {
            command.env_remove("HOME").env_remove("USERPROFILE");
        }
        let mut session = Session::spawn(command).unwrap();
        session.set_expect_timeout(Some(Duration::from_secs(20)));
        session.expect("Select targets to install to").unwrap();
        session.send(target).unwrap();
        session.send(" ").unwrap();
        session.send("\r").unwrap();
        session.expect("Global skills installed").unwrap();
        session.expect(Eof).unwrap();
        let destination = if target == "pi" {
            home.join(".pi/agent/skills/demo")
        } else {
            codex.join("skills/demo")
        };
        assert!(destination.join("SKILL.md").is_file());
        assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
    }
}

#[test]
fn global_target_selection_validates_a_selected_override_before_writing() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["demo"]);
    let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
    command
        .current_dir(&project)
        .args([
            "skill",
            "add",
            repository.to_str().unwrap(),
            "--global",
            "--all",
        ])
        .env("CODEX_HOME", "invalid-relative-override");
    let mut session = Session::spawn(command).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(20)));
    session.expect("Select targets to install to").unwrap();
    session.send("codex").unwrap();
    session.send(" ").unwrap();
    session.send("\r").unwrap();
    session
        .expect("CODEX_HOME must be an absolute path")
        .unwrap();
    session.expect(Eof).unwrap();
    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
}

#[test]
fn standalone_mcp_target_multiselect_installs_checked_target() {
    let project = tempfile::tempdir().unwrap();

    let mut session = standalone_interactive_mcp_add(project.path());
    session.expect("Select targets to install to").unwrap();
    session.send("codex").unwrap();
    session.send(" ").unwrap();
    session.send("\r").unwrap();
    session.expect(Eof).unwrap();

    assert!(project.path().join(".codex/config.toml").is_file());
    assert!(!project.path().join(".mcp.json").exists());
    assert!(!project.path().join("aru.toml").exists());
    assert!(!project.path().join(".aru").exists());
}

#[test]
fn standalone_mcp_target_cancel_writes_nothing() {
    let project = tempfile::tempdir().unwrap();

    let mut session = standalone_interactive_mcp_add(project.path());
    session.expect("Select targets to install to").unwrap();
    session.send(ControlCode::ESC).unwrap();
    session.expect("Target selection canceled").unwrap();
    session.expect(Eof).unwrap();

    assert_eq!(std::fs::read_dir(project.path()).unwrap().count(), 0);
}

#[test]
fn standalone_target_and_skill_multiselect_install_checked_combination() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha", "beta"]);

    let mut session = standalone_interactive_add(&project, &repository);
    session.expect("Select targets to install to").unwrap();
    session.send("codex").unwrap();
    session.send(" ").unwrap();
    session.send("\r").unwrap();
    session.expect("Select skills to install").unwrap();
    session.send("beta").unwrap();
    session.send(" ").unwrap();
    session.send("\r").unwrap();
    session.expect(Eof).unwrap();

    assert!(!project.join(".agents/skills/alpha").exists());
    assert!(project.join(".agents/skills/beta/SKILL.md").is_file());
    assert!(!project.join("aru.toml").exists());
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru").exists());
}

#[test]
fn standalone_target_cancel_writes_nothing() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha"]);

    let mut session = standalone_interactive_add(&project, &repository);
    session.expect("Select targets to install to").unwrap();
    session.send(ControlCode::ESC).unwrap();
    session.expect("Target selection canceled").unwrap();
    session.expect(Eof).unwrap();

    assert_eq!(std::fs::read_dir(&project).unwrap().count(), 0);
}

#[test]
#[ignore = "requires public Git network"]
fn public_interactive_git_select_and_cancel_smoke() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    let source = Path::new("narumiruna/skills");

    let mut select = interactive_add(&project, source, &[]);
    select.expect("Select skills to install").unwrap();
    select.send("designing-user-experiences").unwrap();
    select.send(" ").unwrap();
    select.send("\r").unwrap();
    select.expect(Eof).unwrap();
    assert!(
        project
            .join(".agents/skills/designing-user-experiences")
            .is_dir()
    );
    let parsed_lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(parsed_lock.skill_packages[0].requirement, "version:*");
    assert_eq!(parsed_lock.skill_packages[0].version, "main");
    assert_eq!(parsed_lock.skill_packages[0].revision.len(), 40);

    let manifest = std::fs::read(project.join("aru.toml")).unwrap();
    let lock = std::fs::read(project.join("aru.lock")).unwrap();
    let mut cancel = interactive_add(&project, source, &[]);
    cancel.expect("Select skills to install").unwrap();
    cancel.send(ControlCode::ESC).unwrap();
    cancel.expect("Skill selection canceled").unwrap();
    cancel.expect(Eof).unwrap();
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert_eq!(std::fs::read(project.join("aru.lock")).unwrap(), lock);
}

#[test]
fn terminal_multiselect_filters_and_installs_only_the_checked_skill() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha", "beta", "zeta"]);
    git(&repository, &["branch", "live"]);
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();

    let mut session = interactive_add(&project, &repository, &["--branch", "live"]);
    session.expect("Select skills to install").unwrap();
    session.send("\r").unwrap();
    session.expect("select at least one skill").unwrap();
    session.send("beta").unwrap();
    session.send(" ").unwrap();
    session.send("\r").unwrap();
    session.expect(Eof).unwrap();

    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("include = [\"beta\"]"));
    assert!(manifest.contains("branch = \"live\""));
    assert!(!manifest.contains("include = [\"*\"]"));
    assert!(!project.join(".agents/skills/alpha").exists());
    assert!(project.join(".agents/skills/beta").is_dir());
    assert!(!project.join(".agents/skills/zeta").exists());
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(lock.skill_packages[0].skills[0].name, "beta");
    aru(&project).args(["sync", "--locked"]).assert().success();
}

#[test]
fn terminal_escape_cancels_without_project_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha", "beta"]);
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    let before = std::fs::read(project.join("aru.toml")).unwrap();

    let mut session = interactive_add(&project, &repository, &[]);
    session.expect("Select skills to install").unwrap();
    session.send(ControlCode::ESC).unwrap();
    session.expect("Skill selection canceled").unwrap();
    session.expect(Eof).unwrap();

    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/state.toml").exists());
    assert!(!project.join(".agents").exists());
}

#[test]
fn terminal_interrupt_exits_without_project_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha", "beta"]);
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    let before = std::fs::read(project.join("aru.toml")).unwrap();

    let mut session = interactive_add(&project, &repository, &[]);
    session.expect("Select skills to install").unwrap();
    session.send(ControlCode::ETX).unwrap();
    session
        .expect("interactive skill selection was interrupted")
        .unwrap();
    session.expect(Eof).unwrap();

    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/state.toml").exists());
    assert!(!project.join(".agents").exists());
}

#[test]
fn terminal_dry_run_prompts_but_writes_nothing() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&project).unwrap();
    create_repository(&repository, &["alpha", "beta"]);
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    let before = std::fs::read(project.join("aru.toml")).unwrap();

    let mut session = interactive_add(&project, &repository, &["--dry-run"]);
    session.expect("Select skills to install").unwrap();
    session.send(" ").unwrap();
    session.send("\r").unwrap();
    session.expect("lock skill alpha").unwrap();
    session.expect(Eof).unwrap();

    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".aru/cache").exists());
    assert!(!project.join(".aru/state.toml").exists());
    assert!(!project.join(".agents").exists());
}
