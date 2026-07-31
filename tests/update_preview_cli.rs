use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use walkdir::WalkDir;

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
    assert!(output.status.success(), "git {arguments:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn create_repository(repository: &Path) {
    std::fs::create_dir(repository).unwrap();
    git(repository, &["init", "--quiet"]);
    git(repository, &["config", "user.email", "preview@example.com"]);
    git(repository, &["config", "user.name", "preview tests"]);
    git(repository, &["config", "commit.gpgsign", "false"]);
    let skill = repository.join("skills/demo");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Preview test\n---\n# Demo\n",
    )
    .unwrap();
    git(repository, &["add", "."]);
    git(repository, &["commit", "--quiet", "-m", "initial"]);
    git(repository, &["tag", "1.0.0"]);
}

fn add_version(repository: &Path) {
    std::fs::write(repository.join("skills/demo/version.md"), "1.1.0\n").unwrap();
    git(repository, &["add", "."]);
    git(repository, &["commit", "--quiet", "-m", "1.1.0"]);
    git(repository, &["tag", "1.1.0"]);
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| {
            (
                entry.path().strip_prefix(root).unwrap().to_path_buf(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn skill_update_dry_run_reports_unchanged_and_new_candidates_without_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    create_repository(&repository);
    std::fs::create_dir(&project).unwrap();
    aru(&project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();

    let before = snapshot(&project);
    aru(&project)
        .args(["skill", "update", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Resolved skill repository 1.0.0@"))
        .stderr(predicate::str::contains("(unchanged)"));
    assert_eq!(snapshot(&project), before);

    add_version(&repository);
    aru(&project)
        .args(["skill", "update", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Resolved skill repository 1.0.0@"))
        .stderr(predicate::str::contains("-> 1.1.0@"));
    assert_eq!(snapshot(&project), before);
}

#[test]
fn direct_mcp_update_dry_run_reports_the_unchanged_candidate() {
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
        ])
        .assert()
        .success();
    let before = snapshot(&project);

    aru(&project)
        .args(["mcp", "update", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Resolved MCP docs direct (unchanged)",
        ));

    assert_eq!(snapshot(&project), before);
}
