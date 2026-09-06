use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn git(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn add_skill(repository: &Path, name: &str) {
    let directory = repository.join("skills").join(name);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Test {name}\n---\n# {name}\n"),
    )
    .unwrap();
    git(repository, &["add", "skills"]);
    git(repository, &["commit", "--quiet", "-m", name]);
}

struct Fixture {
    _temporary: tempfile::TempDir,
    repository: PathBuf,
    project: PathBuf,
    home: PathBuf,
    release: String,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        let project = temporary.path().join("project");
        let home = temporary.path().join("home");
        for path in [&repository, &project, &home] {
            std::fs::create_dir(path).unwrap();
        }
        git(
            &repository,
            &["init", "--quiet", "--initial-branch", "trunk"],
        );
        git(
            &repository,
            &["config", "user.email", "refresh@example.com"],
        );
        git(&repository, &["config", "user.name", "refresh tests"]);
        git(&repository, &["config", "commit.gpgsign", "false"]);
        add_skill(&repository, "release");
        git(&repository, &["tag", "1.0.0"]);
        git(&repository, &["branch", "stable"]);
        let release = git(&repository, &["rev-parse", "HEAD"]);
        add_skill(&repository, "alpha");
        Self {
            _temporary: temporary,
            repository,
            project,
            home,
            release,
        }
    }

    fn command(&self, global: bool) -> assert_cmd::Command {
        let mut command = cargo_bin_cmd!("aru");
        command
            .current_dir(&self.project)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .args(["skill", "add", "--target", "pi"]);
        if global {
            command.arg("--global");
        }
        command
    }

    fn destination(&self, global: bool) -> PathBuf {
        if global {
            self.home.join(".pi/agent/skills")
        } else {
            self.project.join(".pi/skills")
        }
    }
}

#[test]
fn standalone_and_global_refresh_default_head_before_preview_and_install() {
    for global in [false, true] {
        let fixture = Fixture::new();
        for name in ["alpha", "beta"] {
            if name == "beta" {
                add_skill(&fixture.repository, name);
            }
            for dry_run in [true, false] {
                let mut command = fixture.command(global);
                command.arg(&fixture.repository).args(["--skill", name]);
                if dry_run {
                    command.arg("--dry-run");
                }
                command.assert().success();
                assert_eq!(
                    fixture
                        .destination(global)
                        .join(name)
                        .join("SKILL.md")
                        .is_file(),
                    !dry_run
                );
            }
        }
        assert!(!fixture.destination(global).join("release").exists());
        assert!(!fixture.project.join("aru.toml").exists());
        assert!(!fixture.project.join("aru.lock").exists());
        assert!(!fixture.project.join(".aru").exists());
    }
}

#[test]
fn standalone_and_global_honor_explicit_references_instead_of_default_head() {
    for global in [false, true] {
        for option in ["--version", "--branch", "--rev"] {
            let fixture = Fixture::new();
            let reference = match option {
                "--version" => "=1.0.0",
                "--branch" => "stable",
                _ => &fixture.release,
            };
            fixture
                .command(global)
                .arg(&fixture.repository)
                .args(["--all", option, reference])
                .assert()
                .success();
            assert!(
                fixture
                    .destination(global)
                    .join("release/SKILL.md")
                    .is_file()
            );
            assert!(!fixture.destination(global).join("alpha").exists());
        }
    }
}

#[test]
fn standalone_and_global_fail_without_default_head_instead_of_using_a_tag() {
    for global in [false, true] {
        let fixture = Fixture::new();
        git(
            &fixture.repository,
            &["symbolic-ref", "HEAD", "refs/heads/missing"],
        );
        fixture
            .command(global)
            .arg(&fixture.repository)
            .arg("--all")
            .assert()
            .failure()
            .stderr(predicate::str::contains("Git source has no default HEAD"));
        assert!(!fixture.destination(global).exists());
        assert!(!fixture.project.join(".aru").exists());
    }
}

#[test]
fn standalone_and_global_offline_reject_remote_head_but_allow_local_head() {
    for global in [false, true] {
        let fixture = Fixture::new();
        fixture
            .command(global)
            .args([
                "https://skills.invalid/repository.git",
                "--all",
                "--offline",
            ])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "offline mode cannot resolve remote",
            ));
        assert!(!fixture.destination(global).exists());
        fixture
            .command(global)
            .arg(&fixture.repository)
            .args(["--all", "--offline"])
            .assert()
            .success();
        assert!(fixture.destination(global).join("alpha/SKILL.md").is_file());
    }
}

#[test]
fn managed_skill_add_still_prefers_tags_and_reuses_the_lock() {
    let fixture = Fixture::new();
    cargo_bin_cmd!("aru")
        .current_dir(&fixture.project)
        .args(["init", "--target", "pi"])
        .assert()
        .success();
    fixture
        .command(false)
        .arg(&fixture.repository)
        .arg("--all")
        .assert()
        .success();
    let locked = std::fs::read(fixture.project.join("aru.lock")).unwrap();
    git(&fixture.repository, &["tag", "2.0.0"]);
    fixture
        .command(false)
        .arg(&fixture.repository)
        .arg("--all")
        .assert()
        .success();
    assert_eq!(
        std::fs::read(fixture.project.join("aru.lock")).unwrap(),
        locked
    );
    assert!(
        fixture
            .destination(false)
            .join("release/SKILL.md")
            .is_file()
    );
    assert!(!fixture.destination(false).join("alpha").exists());
}
