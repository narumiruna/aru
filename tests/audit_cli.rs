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

fn init(project: &Path) {
    std::fs::create_dir(project).unwrap();
    aru(project)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
}

fn add_direct_mcp(project: &Path) {
    aru(project)
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
}

fn git(repository: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success(), "git {arguments:?} failed");
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut output = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.unwrap();
        if entry.path() == root {
            continue;
        }
        let relative = entry.path().strip_prefix(root).unwrap().to_path_buf();
        if entry.file_type().is_symlink() {
            output.insert(
                relative,
                std::fs::read_link(entry.path())
                    .unwrap()
                    .to_string_lossy()
                    .as_bytes()
                    .to_vec(),
            );
        } else if entry.file_type().is_file() {
            output.insert(relative, std::fs::read(entry.path()).unwrap());
        }
    }
    output
}

#[test]
fn audit_help_and_clean_project_are_read_only() {
    cargo_bin_cmd!("aru")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("audit"));
    cargo_bin_cmd!("aru")
        .args(["audit", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--output"));

    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    add_direct_mcp(&project);
    let before = snapshot(&project);

    aru(&project)
        .arg("audit")
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains("Audit passed; no findings."));

    assert_eq!(snapshot(&project), before);
}

#[test]
fn audit_reports_missing_lock_and_pending_recovery_without_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    std::fs::write(
        project.join(".aru/transaction.toml"),
        "version = 1\nphase = \"prepared\"\nentries = []\n",
    )
    .unwrap();
    let before = snapshot(&project);

    aru(&project)
        .arg("audit")
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("[lock.missing]"))
        .stderr(predicate::str::contains("[transaction.pending]"))
        .stderr(predicate::str::contains("error: command reported").not());

    assert_eq!(snapshot(&project), before);
}

#[test]
fn audit_reports_projection_drift_and_hidden_unicode() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    std::fs::write(
        project.join("AGENTS.md"),
        "safe heading\ntext with hidden override \u{202e}abc\n",
    )
    .unwrap();
    aru(&project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();
    add_direct_mcp(&project);
    let config = project.join(".codex/config.toml");
    let changed = std::fs::read_to_string(&config)
        .unwrap()
        .replace("https://example.com/mcp", "https://drift.example/mcp");
    std::fs::write(&config, changed).unwrap();
    let before = snapshot(&project);

    aru(&project)
        .arg("audit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("[content.hidden-unicode]"))
        .stderr(predicate::str::contains("AGENTS.md:2"))
        .stderr(predicate::str::contains("[projection.invalid]"))
        .stderr(predicate::str::contains("drift"));

    assert_eq!(snapshot(&project), before);
}

#[test]
fn audit_scans_deployed_skill_text_for_hidden_controls() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("repository");
    let project = temporary.path().join("project");
    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--quiet"]);
    git(&repository, &["config", "user.email", "audit@example.com"]);
    git(&repository, &["config", "user.name", "audit tests"]);
    git(&repository, &["config", "commit.gpgsign", "false"]);
    std::fs::create_dir_all(repository.join("skills/hidden")).unwrap();
    std::fs::write(
        repository.join("skills/hidden/SKILL.md"),
        "---\nname: hidden\ndescription: hidden control\n---\n# Hidden \u{200b}text\n",
    )
    .unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "initial"]);
    git(&repository, &["tag", "1.0.0"]);
    init(&project);
    aru(&project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();

    aru(&project)
        .arg("audit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("[content.hidden-unicode]"))
        .stderr(predicate::str::contains(".agents/skills/hidden/SKILL.md:5"))
        .stderr(predicate::str::contains("U+200B"));
}

#[test]
fn audit_json_is_deterministic_stdout_clean_and_versioned() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);

    let first = aru(&project)
        .args(["audit", "--format", "json"])
        .output()
        .unwrap();
    let second = aru(&project)
        .args(["audit", "--format", "json"])
        .output()
        .unwrap();
    assert!(!first.status.success());
    assert_eq!(first.stdout, second.stdout);
    let report: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(report["version"], 1);
    assert_eq!(report["status"], "failed");
    assert_eq!(report["findings"][0]["code"], "lock.missing");
    assert!(!String::from_utf8(first.stderr).unwrap().contains("error:"));
}

#[test]
fn audit_can_write_an_explicit_json_output_file() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let output = temporary.path().join("audit.json");
    init(&project);

    aru(&project)
        .args([
            "audit",
            "--format",
            "json",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("Wrote audit report"));

    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(report["version"], 1);
    assert_eq!(report["findings"][0]["code"], "lock.missing");
}

#[test]
fn audit_accepts_multilingual_and_emoji_content() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    std::fs::write(project.join("AGENTS.md"), "繁體中文規則 🧭\n日本語\n").unwrap();
    aru(&project)
        .args(["instruction", "add", "AGENTS.md"])
        .assert()
        .success();

    aru(&project)
        .arg("audit")
        .assert()
        .success()
        .stderr(predicate::str::contains("Audit passed"));
}
