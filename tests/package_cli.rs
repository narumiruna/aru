use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

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
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn init_git(repository: &Path) {
    std::fs::create_dir_all(repository).unwrap();
    git(repository, &["init", "--quiet"]);
    git(
        repository,
        &["config", "user.email", "packages@example.com"],
    );
    git(repository, &["config", "user.name", "package tests"]);
    git(repository, &["config", "commit.gpgsign", "false"]);
}

fn commit_version(repository: &Path, version: &str) {
    git(repository, &["add", "."]);
    git(repository, &["commit", "--quiet", "-m", version]);
    git(repository, &["tag", version]);
}

fn write_package(
    repository: &Path,
    name: &str,
    version: &str,
    instruction: bool,
    skill: Option<&str>,
    mcp: Option<&str>,
    dependencies: &[(&str, &str)],
) {
    let mut manifest = format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n");
    if instruction {
        manifest.push_str(
            "\n[[instructions.sources]]\nfiles = [\"AGENTS.md\"]\nscope = \"source-directory\"\n",
        );
        std::fs::write(
            repository.join("AGENTS.md"),
            format!("# Package {name}\n\nManaged package rules.\n"),
        )
        .unwrap();
    }
    if let Some(skill) = skill {
        manifest.push_str(&format!("\n[skills]\n{skill} = \"skills/{skill}\"\n"));
        std::fs::create_dir_all(repository.join("skills").join(skill)).unwrap();
        std::fs::write(
            repository.join("skills").join(skill).join("SKILL.md"),
            format!("---\nname: {skill}\ndescription: Package skill {skill}\n---\n# {skill}\n"),
        )
        .unwrap();
    }
    if let Some(mcp) = mcp {
        manifest.push_str(&format!(
            "\n[mcp.{mcp}]\nurl = \"https://example.com/{mcp}/mcp\"\n"
        ));
    }
    if !dependencies.is_empty() {
        manifest.push_str("\n[dependencies]\n");
        for (source, requirement) in dependencies {
            manifest.push_str(&format!(
                "\"{source}\" = {{ version = \"{requirement}\" }}\n"
            ));
        }
    }
    std::fs::write(repository.join("aru.toml"), manifest).unwrap();
}

fn init_project(project: &Path, targets: &[&str]) {
    std::fs::create_dir(project).unwrap();
    let mut command = aru(project);
    command.arg("init");
    for target in targets {
        command.args(["--target", target]);
    }
    command.assert().success();
}

struct GitDaemon {
    child: Child,
}

impl GitDaemon {
    fn start(root: &Path) -> (Self, u16) {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut child = Command::new("git")
            .args([
                "daemon",
                "--reuseaddr",
                "--export-all",
                "--listen=127.0.0.1",
                &format!("--port={port}"),
                &format!("--base-path={}", root.display()),
                root.to_str().unwrap(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        for _ in 0..100 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                return (Self { child }, port);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = child.kill();
        let _ = child.wait();
        panic!("git daemon did not start");
    }
}

impl Drop for GitDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn publish_bare(source: &Path, root: &Path, name: &str) -> PathBuf {
    let destination = root.join(format!("{name}.git"));
    let output = Command::new("git")
        .args([
            "clone",
            "--quiet",
            "--bare",
            source.to_str().unwrap(),
            destination.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    destination
}

#[test]
fn package_help_exposes_root_lifecycle_contract() {
    cargo_bin_cmd!("aru")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("add"))
        .stdout(predicate::str::contains("remove"))
        .stdout(predicate::str::contains("update"));
    cargo_bin_cmd!("aru")
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--version"))
        .stdout(predicate::str::contains("--target"))
        .stdout(predicate::str::contains("--trust-mcp"))
        .stdout(predicate::str::contains("--no-sync"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--merge"));
    cargo_bin_cmd!("aru")
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--precise"));
}

#[test]
fn direct_package_add_locked_replay_audit_and_remove_compose_primitives() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("agent-kit");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_package(
        &repository,
        "agent-kit",
        "1.2.0",
        true,
        Some("review"),
        None,
        &[],
    );
    commit_version(&repository, "1.2.0");
    init_project(&project, &["codex", "claude"]);
    std::fs::write(project.join("AGENTS.md"), "# Project rules\n").unwrap();

    aru(&project)
        .args([
            "add",
            repository.to_str().unwrap(),
            "--version",
            "=1.2.0",
            "--target",
            "codex",
            "--target",
            "claude",
            "--merge",
        ])
        .assert()
        .success();

    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("[packages]"));
    assert!(manifest.contains("version = \"=1.2.0\""));
    let agents = std::fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(agents.contains("# Project rules"));
    assert!(agents.contains("Managed package rules"));
    assert!(agents.contains("aru:instruction:start"));
    assert!(project.join(".agents/skills/review").is_dir());
    assert!(project.join(".claude/skills/review").exists());

    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(lock.aru_packages.len(), 1);
    assert_eq!(lock.aru_packages[0].name, "agent-kit");
    assert_eq!(lock.aru_packages[0].package_version, "1.2.0");
    assert_eq!(lock.aru_packages[0].dependencies, Vec::<String>::new());
    aru(&project).args(["lock", "--check"]).assert().success();
    aru(&project).args(["sync", "--check"]).assert().success();
    aru(&project).arg("audit").assert().success();

    let source_cache = std::fs::read_dir(project.join(".aru/cache/git"))
        .unwrap()
        .find_map(std::result::Result::ok)
        .unwrap()
        .path();
    let cached_skill = source_cache
        .join(&lock.aru_packages[0].revision)
        .join("content/skills/review/SKILL.md");
    std::fs::write(&cached_skill, "corrupted package cache\n").unwrap();
    aru(&project)
        .args(["--frozen", "sync", "--merge"])
        .assert()
        .success();
    assert!(
        !std::fs::read_to_string(&cached_skill)
            .unwrap()
            .contains("corrupted")
    );

    std::fs::remove_dir_all(project.join(".agents")).unwrap();
    #[cfg(unix)]
    std::fs::remove_file(project.join(".claude/skills/review")).unwrap();
    #[cfg(not(unix))]
    std::fs::remove_dir_all(project.join(".claude/skills/review")).unwrap();
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(&project)
        .args(["--frozen", "sync", "--merge"])
        .assert()
        .success();
    assert!(project.join(".agents/skills/review").is_dir());

    aru(&project)
        .args(["remove", repository.to_str().unwrap()])
        .assert()
        .success();
    assert!(!project.join(".agents/skills/review").exists());
    assert!(!project.join(".claude/skills/review").exists());
    let agents = std::fs::read_to_string(project.join("AGENTS.md")).unwrap();
    assert!(agents.starts_with("# Project rules\n"));
    assert!(!agents.contains("Managed package rules"));
    assert!(!agents.contains("aru:instruction:"));
}

#[test]
fn package_skills_support_skill_only_targets_and_locked_replay() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("kit");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_package(&repository, "kit", "1.0.0", false, Some("demo"), None, &[]);
    commit_version(&repository, "1.0.0");
    init_project(&project, &["kiro-cli"]);

    aru(&project)
        .args(["add", repository.to_str().unwrap(), "--version", "=1.0.0"])
        .assert()
        .success();
    assert!(project.join(".kiro/skills/demo/SKILL.md").is_file());
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(lock.aru_packages[0].targets, [aru::manifest::Target::Kiro]);
    assert_eq!(
        lock.skill_packages[0].targets,
        [aru::manifest::Target::Kiro]
    );

    std::fs::remove_dir_all(project.join(".kiro/skills/demo")).unwrap();
    aru(&project).args(["--frozen", "sync"]).assert().success();
    assert!(project.join(".kiro/skills/demo/SKILL.md").is_file());
}

#[test]
fn package_add_dry_run_and_no_sync_have_no_hidden_projection_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("kit");
    let dry_project = temporary.path().join("dry-project");
    let deferred_project = temporary.path().join("deferred-project");
    init_git(&repository);
    write_package(&repository, "kit", "1.0.0", false, Some("demo"), None, &[]);
    commit_version(&repository, "1.0.0");

    init_project(&dry_project, &["codex"]);
    let before = std::fs::read(dry_project.join("aru.toml")).unwrap();
    aru(&dry_project)
        .args(["add", repository.to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would lock aru package kit"));
    assert_eq!(std::fs::read(dry_project.join("aru.toml")).unwrap(), before);
    assert!(!dry_project.join("aru.lock").exists());
    assert!(!dry_project.join(".aru/cache").exists());
    assert!(!dry_project.join(".agents").exists());

    init_project(&deferred_project, &["codex"]);
    aru(&deferred_project)
        .args(["add", repository.to_str().unwrap(), "--no-sync"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Target paths were not changed"));
    assert!(deferred_project.join("aru.lock").is_file());
    assert!(!deferred_project.join(".agents").exists());
    aru(&deferred_project).arg("sync").assert().success();
    assert!(deferred_project.join(".agents/skills/demo").is_dir());
}

#[test]
fn failed_package_projection_rolls_back_manifest_lock_and_outputs() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("kit");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_package(&repository, "kit", "1.0.0", false, Some("demo"), None, &[]);
    commit_version(&repository, "1.0.0");
    init_project(&project, &["codex"]);
    std::fs::write(project.join(".agents"), "blocking file").unwrap();
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["add", repository.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Project synchronized").not());
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());
    assert_eq!(
        std::fs::read_to_string(project.join(".agents")).unwrap(),
        "blocking file"
    );
}

#[test]
fn package_update_is_conservative_selective_precise_and_dry_run_safe() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("kit");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_package(&repository, "kit", "1.0.0", false, Some("demo"), None, &[]);
    commit_version(&repository, "1.0.0");
    init_project(&project, &["codex"]);
    aru(&project)
        .args(["add", repository.to_str().unwrap()])
        .assert()
        .success();

    write_package(&repository, "kit", "1.1.0", false, Some("demo"), None, &[]);
    std::fs::write(repository.join("skills/demo/version.md"), "1.1.0\n").unwrap();
    commit_version(&repository, "1.1.0");
    aru(&project).arg("sync").assert().success();
    let conservative = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(conservative.aru_packages[0].package_version, "1.0.0");
    let before = std::fs::read(project.join("aru.lock")).unwrap();

    aru(&project)
        .args(["update", "--dry-run"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Resolved aru package kit 1.0.0@"))
        .stderr(predicate::str::contains("-> 1.1.0@"));
    assert_eq!(std::fs::read(project.join("aru.lock")).unwrap(), before);

    aru(&project).arg("update").assert().success();
    let updated = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(updated.aru_packages[0].package_version, "1.1.0");

    aru(&project)
        .args(["update", repository.to_str().unwrap(), "--precise", "1.0.0"])
        .assert()
        .success();
    let precise = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(precise.aru_packages[0].package_version, "1.0.0");
}

#[test]
fn package_branch_and_revision_requirements_reuse_exact_locked_commits() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("kit");
    let branch_project = temporary.path().join("branch-project");
    let revision_project = temporary.path().join("revision-project");
    init_git(&repository);
    write_package(&repository, "kit", "1.0.0", false, Some("demo"), None, &[]);
    commit_version(&repository, "1.0.0");
    git(&repository, &["branch", "live"]);
    let first_revision = git(&repository, &["rev-parse", "HEAD"]);

    init_project(&branch_project, &["codex"]);
    aru(&branch_project)
        .args(["add", repository.to_str().unwrap(), "--branch", "live"])
        .assert()
        .success();
    std::fs::write(repository.join("branch-change"), "new\n").unwrap();
    git(&repository, &["add", "."]);
    git(&repository, &["commit", "--quiet", "-m", "branch change"]);
    git(&repository, &["branch", "--force", "live", "HEAD"]);
    let second_revision = git(&repository, &["rev-parse", "HEAD"]);
    aru(&branch_project).arg("sync").assert().success();
    let pinned = aru::lockfile::Lockfile::load_optional(&branch_project)
        .unwrap()
        .unwrap();
    assert_eq!(pinned.aru_packages[0].revision, first_revision);
    aru(&branch_project)
        .args(["update", repository.to_str().unwrap()])
        .assert()
        .success();
    let updated = aru::lockfile::Lockfile::load_optional(&branch_project)
        .unwrap()
        .unwrap();
    assert_eq!(updated.aru_packages[0].revision, second_revision);

    init_project(&revision_project, &["codex"]);
    aru(&revision_project)
        .args([
            "add",
            repository.to_str().unwrap(),
            "--rev",
            &first_revision,
        ])
        .assert()
        .success();
    aru(&revision_project).arg("update").assert().success();
    let exact = aru::lockfile::Lockfile::load_optional(&revision_project)
        .unwrap()
        .unwrap();
    assert_eq!(exact.aru_packages[0].revision, first_revision);
}

#[test]
fn invalid_package_sources_fail_before_project_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let package = temporary.path().join("package");
    let legacy_package = temporary.path().join("legacy-package");
    let raw_skill = temporary.path().join("raw-skill");
    let project = temporary.path().join("project");
    init_git(&package);
    write_package(&package, "rules", "1.0.0", true, None, None, &[]);
    commit_version(&package, "1.0.0");
    init_git(&legacy_package);
    std::fs::write(
        legacy_package.join("aru-package.toml"),
        "[package]\nname = \"legacy\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    commit_version(&legacy_package, "1.0.0");
    init_git(&raw_skill);
    std::fs::create_dir_all(raw_skill.join("skills/demo")).unwrap();
    std::fs::write(
        raw_skill.join("skills/demo/SKILL.md"),
        "---\nname: demo\ndescription: raw\n---\n# Raw\n",
    )
    .unwrap();
    commit_version(&raw_skill, "1.0.0");
    init_project(&project, &["codex"]);
    std::fs::write(project.join("AGENTS.md"), "manual\n").unwrap();
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["add", package.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--merge"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());
    assert_eq!(
        std::fs::read_to_string(project.join("AGENTS.md")).unwrap(),
        "manual\n"
    );

    aru(&project)
        .args(["add", raw_skill.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("aru skill add"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());

    aru(&project)
        .args(["add", legacy_package.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no aru.toml"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());
}

#[test]
fn package_skill_uses_selected_native_target_and_duplicate_export_fails() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("package");
    let duplicate_project = temporary.path().join("duplicate-project");
    let copilot_project = temporary.path().join("copilot-project");
    init_git(&repository);
    write_package(
        &repository,
        "skill-kit",
        "1.0.0",
        false,
        Some("demo"),
        None,
        &[],
    );
    commit_version(&repository, "1.0.0");

    init_project(&copilot_project, &["codex", "copilot"]);
    aru(&copilot_project)
        .args(["add", repository.to_str().unwrap(), "--target", "copilot"])
        .assert()
        .success();
    assert!(copilot_project.join(".github/skills/demo").is_dir());
    assert!(!copilot_project.join(".agents/skills/demo").exists());
    let lock = aru::lockfile::Lockfile::load_optional(&copilot_project)
        .unwrap()
        .unwrap();
    assert_eq!(
        lock.skill_packages[0].targets,
        [aru::manifest::Target::Copilot]
    );

    init_project(&duplicate_project, &["codex"]);
    aru(&duplicate_project)
        .args(["skill", "add", repository.to_str().unwrap(), "--all"])
        .assert()
        .success();
    let before = std::fs::read(duplicate_project.join("aru.toml")).unwrap();
    aru(&duplicate_project)
        .args(["add", repository.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "cannot be both a direct skill source and an aru package",
        ));
    assert_eq!(
        std::fs::read(duplicate_project.join("aru.toml")).unwrap(),
        before
    );
}

#[test]
fn package_mcp_requires_an_explicit_root_trust_decision() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("mcp-kit");
    let project = temporary.path().join("project");
    init_git(&repository);
    write_package(
        &repository,
        "mcp-kit",
        "1.0.0",
        false,
        None,
        Some("docs"),
        &[],
    );
    let package_manifest = repository.join("aru.toml");
    let manifest = std::fs::read_to_string(&package_manifest).unwrap();
    std::fs::write(
        &package_manifest,
        format!("{manifest}\n[mcp.docs.env-http-headers]\nX-API-Key = \"DOCS_API_KEY\"\n"),
    )
    .unwrap();
    commit_version(&repository, "1.0.0");
    init_project(&project, &["codex"]);
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["add", repository.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("untrusted package MCP"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());

    aru(&project)
        .env(
            "DOCS_API_KEY",
            "package-marker-secret-must-not-be-persisted",
        )
        .args(["add", repository.to_str().unwrap(), "--trust-mcp", "docs"])
        .assert()
        .success();
    let manifest = std::fs::read_to_string(project.join("aru.toml")).unwrap();
    assert!(manifest.contains("[package-trust"));
    assert!(manifest.contains("mcp = [\"docs\"]"));
    let lock = std::fs::read_to_string(project.join("aru.lock")).unwrap();
    let codex_path = project.join(".codex/config.toml");
    let codex = std::fs::read_to_string(&codex_path).unwrap();
    assert!(codex.contains("X-API-Key = \"DOCS_API_KEY\""));
    assert!(!lock.contains("package-marker-secret-must-not-be-persisted"));
    assert!(!codex.contains("package-marker-secret-must-not-be-persisted"));

    let drifted = codex.replace("DOCS_API_KEY", "DRIFTED_DOCS_API_KEY");
    std::fs::write(&codex_path, &drifted).unwrap();
    let project_manifest = std::fs::read(project.join("aru.toml")).unwrap();
    let project_lock = std::fs::read(project.join("aru.lock")).unwrap();
    aru(&project)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("drift"));
    assert_eq!(
        std::fs::read(project.join("aru.toml")).unwrap(),
        project_manifest
    );
    assert_eq!(
        std::fs::read(project.join("aru.lock")).unwrap(),
        project_lock
    );
    assert_eq!(std::fs::read_to_string(codex_path).unwrap(), drifted);
}

#[test]
fn transitive_remote_package_replays_from_cache_after_the_source_is_offline() {
    let temporary = tempfile::tempdir().unwrap();
    let server_root = temporary.path().join("server");
    let child = temporary.path().join("child");
    let parent = temporary.path().join("parent");
    let project = temporary.path().join("project");
    std::fs::create_dir(&server_root).unwrap();
    init_git(&child);
    write_package(
        &child,
        "shared-kit",
        "1.0.0",
        false,
        Some("shared"),
        None,
        &[],
    );
    commit_version(&child, "1.0.0");
    publish_bare(&child, &server_root, "shared-kit");
    let (daemon, port) = GitDaemon::start(&server_root);
    let child_url = format!("git://127.0.0.1:{port}/shared-kit.git");

    init_git(&parent);
    write_package(
        &parent,
        "parent-kit",
        "1.0.0",
        false,
        None,
        None,
        &[(&child_url, "=1.0.0")],
    );
    commit_version(&parent, "1.0.0");
    init_project(&project, &["codex"]);

    aru(&project)
        .args(["add", parent.to_str().unwrap()])
        .assert()
        .success();
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert_eq!(lock.aru_packages.len(), 2);
    assert_eq!(lock.aru_packages[0].dependencies.len(), 1);
    assert!(project.join(".agents/skills/shared").is_dir());
    let export = aru(&project)
        .args(["export", "--format", "cyclonedx1.5"])
        .output()
        .unwrap();
    assert!(export.status.success());
    let bom: serde_json::Value = serde_json::from_slice(&export.stdout).unwrap();
    let parent_ref = bom["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == "parent-kit")
        .unwrap()["bom-ref"]
        .as_str()
        .unwrap();
    let shared_ref = bom["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|component| component["name"] == "shared-kit")
        .unwrap()["bom-ref"]
        .as_str()
        .unwrap();
    let parent_dependencies = bom["dependencies"]
        .as_array()
        .unwrap()
        .iter()
        .find(|dependency| dependency["ref"] == parent_ref)
        .unwrap()["dependsOn"]
        .as_array()
        .unwrap();
    assert!(
        parent_dependencies
            .iter()
            .any(|dependency| dependency == shared_ref)
    );

    drop(daemon);
    std::fs::remove_dir_all(project.join(".agents")).unwrap();
    std::fs::remove_file(project.join(".aru/state.toml")).unwrap();
    aru(&project).args(["--frozen", "sync"]).assert().success();
    assert!(project.join(".agents/skills/shared").is_dir());
    aru(&project).arg("audit").assert().success();
}

#[test]
fn shared_transitive_package_unions_compatible_target_reach() {
    let temporary = tempfile::tempdir().unwrap();
    let server_root = temporary.path().join("server");
    let shared = temporary.path().join("shared");
    let left = temporary.path().join("left");
    let right = temporary.path().join("right");
    let project = temporary.path().join("project");
    std::fs::create_dir(&server_root).unwrap();
    init_git(&shared);
    write_package(&shared, "shared", "1.0.0", false, Some("shared"), None, &[]);
    commit_version(&shared, "1.0.0");
    publish_bare(&shared, &server_root, "shared");
    let (_daemon, port) = GitDaemon::start(&server_root);
    let shared_url = format!("git://127.0.0.1:{port}/shared.git");
    for (repository, name) in [(&left, "left"), (&right, "right")] {
        init_git(repository);
        write_package(
            repository,
            name,
            "1.0.0",
            false,
            None,
            None,
            &[(&shared_url, "=1.0.0")],
        );
        commit_version(repository, "1.0.0");
    }
    init_project(&project, &["codex", "claude"]);
    aru(&project)
        .args(["add", left.to_str().unwrap(), "--target", "codex"])
        .assert()
        .success();
    aru(&project)
        .args(["add", right.to_str().unwrap(), "--target", "claude"])
        .assert()
        .success();

    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    let shared = lock
        .aru_packages
        .iter()
        .find(|package| package.name == "shared")
        .unwrap();
    assert_eq!(
        shared.targets,
        [aru::manifest::Target::Codex, aru::manifest::Target::Claude]
    );
    assert_eq!(
        lock.aru_packages
            .iter()
            .filter(|package| package.name == "shared")
            .count(),
        1
    );
    assert!(project.join(".agents/skills/shared").is_dir());
    assert!(project.join(".claude/skills/shared").exists());
}

#[test]
fn conflicting_shared_requirements_and_ambiguous_package_names_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let server_root = temporary.path().join("server");
    let shared = temporary.path().join("shared");
    let left = temporary.path().join("left");
    let conflict = temporary.path().join("conflict");
    let duplicate_name = temporary.path().join("duplicate-name");
    let project = temporary.path().join("project");
    std::fs::create_dir(&server_root).unwrap();
    init_git(&shared);
    write_package(&shared, "shared", "1.0.0", false, None, None, &[]);
    commit_version(&shared, "1.0.0");
    publish_bare(&shared, &server_root, "shared");
    let (_daemon, port) = GitDaemon::start(&server_root);
    let shared_url = format!("git://127.0.0.1:{port}/shared.git");
    init_git(&left);
    write_package(
        &left,
        "left",
        "1.0.0",
        false,
        None,
        None,
        &[(&shared_url, "=1.0.0")],
    );
    commit_version(&left, "1.0.0");
    init_git(&conflict);
    write_package(
        &conflict,
        "conflict",
        "1.0.0",
        false,
        None,
        None,
        &[(&shared_url, "^1.0")],
    );
    commit_version(&conflict, "1.0.0");
    init_git(&duplicate_name);
    write_package(&duplicate_name, "left", "1.0.0", false, None, None, &[]);
    commit_version(&duplicate_name, "1.0.0");
    init_project(&project, &["codex"]);
    aru(&project)
        .args(["add", left.to_str().unwrap()])
        .assert()
        .success();
    let before = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["add", conflict.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("conflicting requirements"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);

    aru(&project)
        .args(["add", duplicate_name.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "package name \"left\" is provided by more than one source",
        ));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), before);
}

#[test]
fn transitive_cycle_is_rejected_before_project_writes() {
    let temporary = tempfile::tempdir().unwrap();
    let server_root = temporary.path().join("server");
    let first = temporary.path().join("first");
    let second = temporary.path().join("second");
    let project = temporary.path().join("project");
    std::fs::create_dir(&server_root).unwrap();
    let (_daemon, port) = GitDaemon::start(&server_root);
    let first_url = format!("git://127.0.0.1:{port}/first.git");
    let second_url = format!("git://127.0.0.1:{port}/second.git");
    init_git(&first);
    write_package(
        &first,
        "first",
        "1.0.0",
        false,
        None,
        None,
        &[(&second_url, "=1.0.0")],
    );
    commit_version(&first, "1.0.0");
    init_git(&second);
    write_package(
        &second,
        "second",
        "1.0.0",
        false,
        None,
        None,
        &[(&first_url, "=1.0.0")],
    );
    commit_version(&second, "1.0.0");
    publish_bare(&first, &server_root, "first");
    publish_bare(&second, &server_root, "second");
    init_project(&project, &["codex"]);
    let manifest = std::fs::read(project.join("aru.toml")).unwrap();

    aru(&project)
        .args(["add", &first_url])
        .assert()
        .failure()
        .stderr(predicate::str::contains("dependency cycle"));
    assert_eq!(std::fs::read(project.join("aru.toml")).unwrap(), manifest);
    assert!(!project.join("aru.lock").exists());
    assert!(!project.join(".agents").exists());
}

#[test]
fn transitive_local_dependencies_and_oversized_graphs_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let child = temporary.path().join("child");
    let parent = temporary.path().join("parent");
    let large = temporary.path().join("large");
    let local_project = temporary.path().join("local-project");
    let large_project = temporary.path().join("large-project");
    init_git(&child);
    write_package(&child, "child", "1.0.0", false, None, None, &[]);
    commit_version(&child, "1.0.0");
    init_git(&parent);
    write_package(
        &parent,
        "parent",
        "1.0.0",
        false,
        None,
        None,
        &[(child.to_str().unwrap(), "=1.0.0")],
    );
    commit_version(&parent, "1.0.0");
    init_project(&local_project, &["codex"]);
    aru(&local_project)
        .args(["add", parent.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("transitive local package"));
    assert!(!local_project.join("aru.lock").exists());

    init_git(&large);
    let dependencies = (0..513)
        .map(|index| {
            (
                format!("https://example.com/package-{index}.git"),
                "=1.0.0".to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let borrowed = dependencies
        .iter()
        .map(|(source, requirement)| (source.as_str(), requirement.as_str()))
        .collect::<Vec<_>>();
    write_package(&large, "large", "1.0.0", false, None, None, &borrowed);
    commit_version(&large, "1.0.0");
    init_project(&large_project, &["codex"]);
    aru(&large_project)
        .args(["add", large.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("512 package dependency edges"));
    assert!(!large_project.join("aru.lock").exists());
}
