use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

fn add_mcp(project: &Path) {
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
fn export_help_and_missing_lock_are_explicit() {
    cargo_bin_cmd!("aru")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("export"));
    cargo_bin_cmd!("aru")
        .args(["export", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cyclonedx1.5"))
        .stdout(predicate::str::contains("--output-file"))
        .stdout(predicate::str::contains("--timestamp"));

    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    aru(&project)
        .args(["export", "--format", "cyclonedx1.5"])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("requires an existing aru.lock"));
}

#[test]
fn cyclonedx_export_is_deterministic_offline_and_read_only() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    add_mcp(&project);
    let before = snapshot(&project);

    let first = aru(&project)
        .args([
            "--offline",
            "export",
            "--format",
            "cyclonedx1.5",
            "--timestamp",
            "2026-07-31T00:00:00Z",
        ])
        .output()
        .unwrap();
    let second = aru(&project)
        .args([
            "--offline",
            "export",
            "--format",
            "cyclonedx1.5",
            "--timestamp",
            "2026-07-31T00:00:00Z",
        ])
        .output()
        .unwrap();
    assert!(first.status.success());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(
        first.stdout,
        include_bytes!("fixtures/contracts/cyclonedx-1.5.json")
    );
    assert_eq!(snapshot(&project), before);

    let bom: serde_json::Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(bom["bomFormat"], "CycloneDX");
    assert_eq!(bom["specVersion"], "1.5");
    assert_eq!(bom["version"], 1);
    assert_eq!(bom["metadata"]["timestamp"], "2026-07-31T00:00:00Z");
    assert_eq!(bom["metadata"]["properties"][0]["value"], "inventory");
    assert_eq!(bom["components"][0]["name"], "docs");
    assert_eq!(bom["dependencies"][0]["ref"], "aru:root");
}

#[test]
fn export_scrubs_url_credentials_and_rejects_unexportable_urls() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    init(&project);
    add_mcp(&project);
    let mut lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    lock.mcp_servers[0].registry = Some("https://user:secret@example.com/registry".into());
    std::fs::write(project.join("aru.lock"), lock.bytes().unwrap()).unwrap();

    aru(&project)
        .args(["export", "--format", "cyclonedx1.5"])
        .assert()
        .success()
        .stdout(predicate::str::contains("https://example.com/registry"))
        .stdout(predicate::str::contains("secret").not())
        .stdout(predicate::str::contains("user@").not());

    lock.mcp_servers[0].registry = Some("not a URL".into());
    std::fs::write(project.join("aru.lock"), lock.bytes().unwrap()).unwrap();
    aru(&project)
        .args(["export", "--format", "cyclonedx1.5"])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("cannot export MCP registry URL"));
    aru(&project)
        .arg("audit")
        .assert()
        .failure()
        .stderr(predicate::str::contains("[export.invalid]"));
}

#[test]
fn export_writes_only_an_explicit_output_file_and_validates_timestamp() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let output = temporary.path().join("bom.json");
    init(&project);
    add_mcp(&project);

    aru(&project)
        .args([
            "export",
            "--format",
            "cyclonedx1.5",
            "--output-file",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains("Wrote CycloneDX inventory"));
    let bom: serde_json::Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(bom["bomFormat"], "CycloneDX");

    aru(&project)
        .args(["export", "--format", "cyclonedx1.5", "--timestamp", "today"])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("RFC 3339 UTC timestamp"));
}
