use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn package(
    source: &str,
    name: &str,
    targets: Vec<aru::manifest::Target>,
    dependencies: Vec<String>,
) -> aru::lockfile::AruPackage {
    aru::lockfile::AruPackage {
        source: source.into(),
        requirement: "version:^1.0".into(),
        version: "1.0.0".into(),
        revision: "0123456789abcdef0123456789abcdef01234567".into(),
        name: name.into(),
        package_version: "1.0.0".into(),
        manifest_sha256: format!("sha256:{name}-manifest"),
        content_sha256: format!("sha256:{name}-content"),
        targets,
        dependencies,
        instruction_sources: Vec::new(),
        skills: Vec::new(),
        mcp: Vec::new(),
    }
}

fn graph_project(project: &Path) {
    std::fs::write(
        project.join("aru.toml"),
        "[project]\ntargets = [\"codex\", \"claude\"]\n",
    )
    .unwrap();
    let shared = "git+https://example.com/shared.git";
    let mut lock = aru::lockfile::Lockfile::empty();
    lock.package_input_hash = "sha256:input".into();
    lock.projection_input_hash = "sha256:projection".into();
    lock.aru_packages = vec![
        package(
            "git+https://user:secret@example.com/alpha.git",
            "alpha",
            vec![aru::manifest::Target::Codex, aru::manifest::Target::Claude],
            vec![shared.into()],
        ),
        package(
            "git+https://example.com/beta.git",
            "beta",
            vec![aru::manifest::Target::Codex],
            vec![shared.into()],
        ),
        package(
            shared,
            "shared",
            vec![aru::manifest::Target::Codex, aru::manifest::Target::Claude],
            Vec::new(),
        ),
    ];
    std::fs::write(project.join("aru.lock"), lock.bytes().unwrap()).unwrap();
}

fn git(repository: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn inspection_and_distribution_help_expose_stable_command_names() {
    cargo_bin_cmd!("aru")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("tree"))
        .stdout(predicate::str::contains("info"))
        .stdout(predicate::str::contains("metadata"))
        .stdout(predicate::str::contains("package"));
    cargo_bin_cmd!("aru")
        .args(["tree", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--invert"))
        .stdout(predicate::str::contains("--target"));
    cargo_bin_cmd!("aru")
        .args(["package", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--allow-dirty"))
        .stdout(predicate::str::contains("--list"));
}

#[test]
fn tree_text_json_depth_target_and_inverse_queries_are_deterministic() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    graph_project(project);

    aru(project).arg("tree").assert().success().stdout(concat!(
        "project\n",
        "├── alpha v1.0.0\n",
        "│   └── shared v1.0.0\n",
        "└── beta v1.0.0\n",
        "    └── shared v1.0.0 (*)\n",
    ));
    aru(project)
        .args(["tree", "--depth", "1"])
        .assert()
        .success()
        .stdout("project\n├── alpha v1.0.0\n└── beta v1.0.0\n");
    aru(project)
        .args(["tree", "--target", "claude"])
        .assert()
        .success()
        .stdout("project\n└── alpha v1.0.0\n    └── shared v1.0.0\n");
    aru(project)
        .args(["tree", "--invert", "shared"])
        .assert()
        .success()
        .stdout(concat!(
            "shared v1.0.0\n",
            "├── alpha v1.0.0\n",
            "└── beta v1.0.0\n",
        ));

    let output = aru(project)
        .args(["tree", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let graph: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(graph["version"], 1);
    assert_eq!(graph["roots"].as_array().unwrap().len(), 2);
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 3);
    assert_eq!(graph["edges"].as_array().unwrap().len(), 2);
    assert!(!String::from_utf8(output.stdout).unwrap().contains("secret"));
    let shallow = aru(project)
        .args(["tree", "--format", "json", "--depth", "1"])
        .output()
        .unwrap();
    let graph: serde_json::Value = serde_json::from_slice(&shallow.stdout).unwrap();
    assert_eq!(graph["nodes"].as_array().unwrap().len(), 2);
    assert!(graph["edges"].as_array().unwrap().is_empty());
}

#[test]
fn empty_tree_and_ambiguous_or_corrupt_graph_states_are_explicit() {
    let empty = tempfile::tempdir().unwrap();
    std::fs::write(
        empty.path().join("aru.toml"),
        "[project]\ntargets = [\"codex\"]\n",
    )
    .unwrap();
    let mut lock = aru::lockfile::Lockfile::empty();
    lock.package_input_hash = "sha256:input".into();
    lock.projection_input_hash = "sha256:projection".into();
    std::fs::write(empty.path().join("aru.lock"), lock.bytes().unwrap()).unwrap();
    aru(empty.path())
        .arg("tree")
        .assert()
        .success()
        .stdout("project\n");

    let graph = tempfile::tempdir().unwrap();
    graph_project(graph.path());
    aru(graph.path())
        .args(["info", "example.com"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous package selector"))
        .stderr(predicate::str::contains("alpha"))
        .stderr(predicate::str::contains("beta"))
        .stderr(predicate::str::contains("secret").not());

    let lock_path = graph.path().join("aru.lock");
    let lock = std::fs::read_to_string(&lock_path).unwrap();
    let corrupt = lock.replace(
        "content-sha256 = \"sha256:shared-content\"\n",
        concat!(
            "content-sha256 = \"sha256:shared-content\"\n",
            "dependencies = [\"git+https://user:secret@example.com/alpha.git\"]\n",
        ),
    );
    assert_ne!(corrupt, lock);
    std::fs::write(&lock_path, corrupt).unwrap();
    aru(graph.path())
        .arg("tree")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cycle").or(predicate::str::contains("invalid")));
}

#[test]
fn info_reports_locked_and_uninstalled_package_metadata() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path().join("project");
    let repository = temporary.path().join("uninstalled");
    std::fs::create_dir(&project).unwrap();
    graph_project(&project);

    aru(&project)
        .args(["--offline", "info", "alpha"])
        .assert()
        .success()
        .stdout(predicate::str::contains("name:         alpha"))
        .stdout(predicate::str::contains("locked:       1.0.0"))
        .stdout(predicate::str::contains("dependencies: 1"))
        .stdout(predicate::str::contains("secret").not());
    aru(&project)
        .args([
            "--offline",
            "info",
            "https://example.invalid/uninstalled.git",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("offline mode cannot inspect"));

    std::fs::create_dir(&repository).unwrap();
    git(&repository, &["init", "--quiet"]);
    git(&repository, &["config", "user.email", "info@example.com"]);
    git(&repository, &["config", "user.name", "info tests"]);
    git(&repository, &["config", "commit.gpgsign", "false"]);
    std::fs::write(
        repository.join("aru.toml"),
        "[package]\nname='uninstalled'\nversion='1.2.0'\n[skills]\nreview='skills/review'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repository.join("skills/review")).unwrap();
    std::fs::write(
        repository.join("skills/review/SKILL.md"),
        "---\nname: review\ndescription: Review\n---\n# Review\n",
    )
    .unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "initial"]);
    git(&repository, &["tag", "1.2.0"]);

    aru(&project)
        .args(["info", repository.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("name:         uninstalled"))
        .stdout(predicate::str::contains("available:    1.2.0"))
        .stdout(predicate::str::contains("skills:       1"));
    assert!(!project.join(".aru").exists());
}

#[test]
fn metadata_requires_a_version_is_credential_free_and_supports_no_deps() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    graph_project(project);

    aru(project)
        .arg("metadata")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--format-version"));
    aru(project)
        .args(["metadata", "--format-version", "2"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unsupported metadata format version",
        ));

    let full = aru(project)
        .args(["metadata", "--format-version", "1"])
        .output()
        .unwrap();
    let repeated = aru(project)
        .args(["metadata", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(full.status.success());
    assert_eq!(full.stdout, repeated.stdout);
    assert!(full.stderr.is_empty());
    let mut metadata: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    assert_eq!(metadata["format_version"], 1);
    assert_eq!(metadata["packages"].as_array().unwrap().len(), 3);
    assert_eq!(metadata["edges"].as_array().unwrap().len(), 2);
    assert!(!String::from_utf8(full.stdout).unwrap().contains("secret"));
    metadata["project_root"] = "<PROJECT>".into();
    let rendered = format!("{}\n", serde_json::to_string_pretty(&metadata).unwrap());
    assert_eq!(
        rendered,
        include_str!("fixtures/contracts/metadata-v1.json")
    );

    let direct = aru(project)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .unwrap();
    assert!(direct.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&direct.stdout).unwrap();
    assert_eq!(metadata["packages"].as_array().unwrap().len(), 2);
    assert!(metadata["edges"].as_array().unwrap().is_empty());
}
