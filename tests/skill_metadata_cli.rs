use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(project: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", project.to_str().unwrap()]);
    command
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository)
        .env("GIT_AUTHOR_NAME", "aru test")
        .env("GIT_AUTHOR_EMAIL", "aru-test@example.invalid")
        .env("GIT_COMMITTER_NAME", "aru test")
        .env("GIT_COMMITTER_EMAIL", "aru-test@example.invalid")
        .args(["-c", "commit.gpgsign=false"])
        .args(arguments)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {arguments:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn skill(extra: &str, body: &str) -> String {
    format!("---\nname: review\ndescription: Review code\n{extra}---\n{body}\n")
}

struct Fixture {
    _temporary: tempfile::TempDir,
    project: PathBuf,
    repository: PathBuf,
}

impl Fixture {
    fn new(targets: &[&str]) -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let project = temporary.path().join("project");
        let repository = temporary.path().join("repository");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir_all(repository.join("review")).unwrap();
        git(&repository, &["init", "--quiet", "-b", "main"]);
        std::fs::write(
            repository.join("review/SKILL.md"),
            skill("license: MIT\ncompatibility: old\n", "Original body"),
        )
        .unwrap();
        std::fs::write(repository.join("review/asset.txt"), "original asset").unwrap();
        git(&repository, &["add", "review"]);
        git(
            &repository,
            &["commit", "--quiet", "-m", "test: initial skill"],
        );
        let mut command = aru(&project);
        command.arg("init");
        for target in targets {
            command.args(["--target", target]);
        }
        command.assert().success();
        aru(&project)
            .args([
                "skill",
                "add",
                repository.to_str().unwrap(),
                "--branch",
                "main",
                "--all",
            ])
            .assert()
            .success();
        Self {
            _temporary: temporary,
            project,
            repository,
        }
    }

    fn path(&self) -> PathBuf {
        self.project.join(".pi/skills/review/SKILL.md")
    }

    fn edit(&self) -> String {
        let text = skill(
            "compatibility: old\ndisable-model-invocation: true\nmetadata:\n  category: [local, review]\n",
            "Original body",
        );
        std::fs::write(self.path(), &text).unwrap();
        text
    }

    fn upstream(&self, extra: &str, body: &str) {
        std::fs::write(self.repository.join("review/SKILL.md"), skill(extra, body)).unwrap();
        std::fs::write(self.repository.join("review/asset.txt"), body).unwrap();
        git(
            &self.repository,
            &["add", "review/SKILL.md", "review/asset.txt"],
        );
        git(
            &self.repository,
            &["commit", "--quiet", "-m", "test: update skill"],
        );
    }

    fn legacy_state(&self) {
        let mut state = aru::ownership::State::load(&self.project).unwrap();
        state.version = 1;
        for entry in &mut state.entries {
            entry.skill_metadata = None;
        }
        std::fs::write(
            self.project.join(aru::ownership::STATE_FILE),
            state.bytes().unwrap(),
        )
        .unwrap();
    }

    fn check(&self) {
        aru(&self.project)
            .args(["sync", "--locked", "--offline", "--check"])
            .assert()
            .success();
    }
}

#[test]
fn sync_accepts_local_metadata_without_rewriting_it_or_the_lock() {
    let fixture = Fixture::new(&["pi"]);
    let lock = std::fs::read(fixture.project.join("aru.lock")).unwrap();
    let local = fixture.edit();
    let before = aru::transaction::path_digest(&fixture.project).unwrap();
    aru(&fixture.project)
        .args(["sync", "--locked", "--offline", "--dry-run"])
        .assert()
        .success();
    assert_eq!(
        aru::transaction::path_digest(&fixture.project).unwrap(),
        before
    );
    aru(&fixture.project)
        .args(["sync", "--locked", "--offline"])
        .assert()
        .success();
    assert_eq!(std::fs::read_to_string(fixture.path()).unwrap(), local);
    assert_eq!(
        std::fs::read(fixture.project.join("aru.lock")).unwrap(),
        lock
    );
    assert!(
        !std::fs::read_to_string(fixture.repository.join("review/SKILL.md"))
            .unwrap()
            .contains("disable-model-invocation")
    );
    fixture.check();
    std::fs::remove_dir_all(fixture.project.join(".aru/cache")).unwrap();
    fixture.check();
    aru(&fixture.project).arg("audit").assert().success();
    // A normal sync can repopulate the cache without losing overrides.
    aru(&fixture.project)
        .args(["sync", "--locked"])
        .assert()
        .success();
    let before = aru::transaction::path_digest(&fixture.project).unwrap();
    aru(&fixture.project)
        .args(["sync", "--locked"])
        .assert()
        .success();
    assert_eq!(
        aru::transaction::path_digest(&fixture.project).unwrap(),
        before
    );
}

#[test]
fn update_keeps_local_fields_and_deletions_but_tracks_untouched_upstream_fields() {
    let fixture = Fixture::new(&["pi"]);
    fixture.edit();
    aru(&fixture.project).arg("sync").assert().success();
    // Recorded overrides do not depend on retaining the old cache.
    std::fs::remove_dir_all(fixture.project.join(".aru/cache")).unwrap();
    for (invocation, body) in [
        ("false", "Second body"),
        ("true", "Third body"),
        ("false", "Fourth body"),
    ] {
        fixture.upstream(&format!("license: Apache\ncompatibility: new\ndisable-model-invocation: {invocation}\nmetadata: {{category: upstream}}\nnew-field: upstream\n"), body);
        aru(&fixture.project)
            .args(["skill", "update"])
            .assert()
            .success();
        let installed = std::fs::read_to_string(fixture.path()).unwrap();
        assert!(
            installed.contains("disable-model-invocation: true"),
            "{installed}"
        );
        assert!(!installed.contains("license:"), "{installed}");
        assert!(installed.contains("compatibility: new"), "{installed}");
        assert!(installed.contains("new-field: upstream"), "{installed}");
        assert!(installed.contains("local"), "{installed}");
        assert!(installed.contains(body), "{installed}");
        assert_eq!(
            std::fs::read_to_string(fixture.path().with_file_name("asset.txt")).unwrap(),
            body
        );
        fixture.check();
    }
}

#[test]
fn edits_are_captured_during_update_and_from_legacy_state() {
    for legacy in [false, true] {
        let fixture = Fixture::new(&["pi"]);
        if legacy {
            fixture.legacy_state();
        }
        fixture.edit();
        fixture.upstream(
            "license: Apache\ncompatibility: new\ndisable-model-invocation: false\n",
            "Updated body",
        );
        aru(&fixture.project)
            .args(["skill", "update"])
            .assert()
            .success();
        let installed = std::fs::read_to_string(fixture.path()).unwrap();
        assert!(installed.contains("disable-model-invocation: true"));
        assert!(installed.contains("Updated body"));
        fixture.check();
    }
    let fixture = Fixture::new(&["pi"]);
    fixture.legacy_state();
    fixture.edit();
    aru(&fixture.project)
        .args(["sync", "--locked"])
        .assert()
        .success();
    fixture.check();
}

#[test]
fn native_package_and_subdirectory_plugin_skills_preserve_legacy_metadata_edits() {
    for plugin in [false, true] {
        let fixture = Fixture::new(&["pi"]);
        aru(&fixture.project)
            .args(["skill", "remove", fixture.repository.to_str().unwrap()])
            .assert()
            .success();
        let source = if plugin {
            let root = fixture.repository.join("bundle");
            std::fs::create_dir_all(root.join("skills/review")).unwrap();
            std::fs::write(root.join("plugin.json"), "{\"$schema\":\"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json\",\"name\":\"review-kit\",\"version\":\"1.0.0\"}\n").unwrap();
            let source = root.join("skills/review/SKILL.md");
            std::fs::write(
                &source,
                skill("license: MIT\ncompatibility: old\n", "Original body"),
            )
            .unwrap();
            git(&fixture.repository, &["add", "bundle"]);
            source
        } else {
            std::fs::write(fixture.repository.join("aru.toml"), "[package]\nname = \"review-kit\"\nversion = \"1.0.0\"\n[skills]\nreview = \"review\"\n").unwrap();
            git(&fixture.repository, &["add", "aru.toml"]);
            fixture.repository.join("review/SKILL.md")
        };
        git(
            &fixture.repository,
            &["commit", "--quiet", "-m", "test: package skill"],
        );
        let mut command = aru(&fixture.project);
        if plugin {
            command.arg("plugin");
        }
        command.args([
            "add",
            fixture.repository.to_str().unwrap(),
            "--branch",
            "main",
        ]);
        if plugin {
            command.args(["--subdir", "bundle", "--component", "skills"]);
        }
        command.assert().success();
        fixture.legacy_state();
        fixture.edit();
        std::fs::write(
            &source,
            skill(
                "disable-model-invocation: false\ncompatibility: new\n",
                "Updated package body",
            ),
        )
        .unwrap();
        git(
            &fixture.repository,
            &[
                "add",
                source
                    .strip_prefix(&fixture.repository)
                    .unwrap()
                    .to_str()
                    .unwrap(),
            ],
        );
        git(
            &fixture.repository,
            &["commit", "--quiet", "-m", "test: update packaged skill"],
        );
        let mut command = aru(&fixture.project);
        if plugin {
            command.arg("plugin");
        }
        command.arg("update").assert().success();
        let installed = std::fs::read_to_string(fixture.path()).unwrap();
        assert!(installed.contains("disable-model-invocation: true"));
        assert!(installed.contains("Updated package body"));
        fixture.check();
    }
}

#[test]
fn deferred_update_and_local_metadata_are_reconciled_without_old_source_cache() {
    let fixture = Fixture::new(&["pi"]);
    fixture.edit();
    aru(&fixture.project).arg("sync").assert().success();
    fixture.upstream("disable-model-invocation: false\n", "Deferred body");
    aru(&fixture.project)
        .args(["skill", "update", "--no-sync"])
        .assert()
        .success();
    aru(&fixture.project)
        .args(["sync", "--check"])
        .assert()
        .failure();
    std::fs::remove_dir_all(fixture.project.join(".aru/cache")).unwrap();
    aru(&fixture.project)
        .args(["sync", "--locked"])
        .assert()
        .success();
    let installed = std::fs::read_to_string(fixture.path()).unwrap();
    assert!(installed.contains("disable-model-invocation: true"));
    assert!(installed.contains("Deferred body"));
    fixture.check();
}

#[test]
fn missing_state_never_adopts_metadata_edits() {
    let fixture = Fixture::new(&["pi"]);
    fixture.edit();
    std::fs::remove_file(fixture.project.join(".aru/state.toml")).unwrap();
    let before = aru::transaction::path_digest(&fixture.project).unwrap();
    aru(&fixture.project)
        .args(["sync", "--locked"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("collision"));
    assert_eq!(
        aru::transaction::path_digest(&fixture.project).unwrap(),
        before
    );
}

#[test]
fn protected_content_and_malformed_metadata_fail_before_any_project_write() {
    let fixture = Fixture::new(&["pi"]);
    let local = fixture.edit();
    aru(&fixture.project).arg("sync").assert().success();
    for invalid in [
        local.replace("name: review", "name: other"),
        local.replace("Review code", "Other description"),
        local.replace("Original body", "Changed body"),
        local.replace(
            "disable-model-invocation: true",
            "disable-model-invocation: [",
        ),
        local.replace(
            "disable-model-invocation: true",
            "disable-model-invocation: true\ndisable-model-invocation: false",
        ),
    ] {
        std::fs::write(fixture.path(), invalid).unwrap();
        let before = aru::transaction::path_digest(&fixture.project).unwrap();
        aru(&fixture.project)
            .args(["sync", "--force"])
            .assert()
            .failure();
        assert_eq!(
            aru::transaction::path_digest(&fixture.project).unwrap(),
            before
        );
    }
    std::fs::write(fixture.path(), &local).unwrap();
    for asset in ["asset.txt", "new-file.txt"] {
        std::fs::write(fixture.path().with_file_name(asset), "changed").unwrap();
        let before = aru::transaction::path_digest(&fixture.project).unwrap();
        aru(&fixture.project)
            .arg("sync")
            .assert()
            .failure()
            .stderr(predicate::str::contains("drift"));
        assert_eq!(
            aru::transaction::path_digest(&fixture.project).unwrap(),
            before
        );
        std::fs::remove_file(fixture.path().with_file_name(asset)).unwrap();
        if asset == "asset.txt" {
            std::fs::write(fixture.path().with_file_name(asset), "original asset").unwrap();
        }
    }
    std::fs::remove_file(fixture.path().with_file_name("asset.txt")).unwrap();
    aru(&fixture.project)
        .arg("sync")
        .assert()
        .failure()
        .stderr(predicate::str::contains("drift"));
}

#[test]
fn removal_preserves_overrides_and_missing_projection_can_be_recreated() {
    let fixture = Fixture::new(&["pi"]);
    fixture.edit();
    aru(&fixture.project).arg("sync").assert().success();
    let before = aru::transaction::path_digest(&fixture.project).unwrap();
    aru(&fixture.project)
        .args(["skill", "remove", fixture.repository.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("local metadata overrides"));
    assert_eq!(
        aru::transaction::path_digest(&fixture.project).unwrap(),
        before
    );
    std::fs::remove_dir_all(fixture.path().parent().unwrap()).unwrap();
    aru(&fixture.project)
        .args(["sync", "--locked"])
        .assert()
        .success();
    assert!(
        std::fs::read_to_string(fixture.path())
            .unwrap()
            .contains("disable-model-invocation: true")
    );
    fixture.check();
    std::fs::remove_dir_all(fixture.path().parent().unwrap()).unwrap();
    aru(&fixture.project)
        .args(["skill", "remove", fixture.repository.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
#[cfg(unix)]
fn shared_symlinks_keep_overrides_through_updates_and_new_targets() {
    // Replit sorts after pi, exercising planning of a later canonical target first.
    let fixture = Fixture::new(&["pi", "replit"]);
    assert!(
        std::fs::symlink_metadata(fixture.path().parent().unwrap())
            .unwrap()
            .file_type()
            .is_symlink()
    );
    fixture.edit();
    aru(&fixture.project).arg("sync").assert().success();
    fixture.upstream(
        "disable-model-invocation: false\ncompatibility: new\n",
        "New body",
    );
    aru(&fixture.project)
        .args(["skill", "update"])
        .assert()
        .success();
    let shared = fixture.project.join(".agents/skills/review/SKILL.md");
    assert_eq!(
        std::fs::read(fixture.path()).unwrap(),
        std::fs::read(&shared).unwrap()
    );
    assert!(
        std::fs::read_to_string(&shared)
            .unwrap()
            .contains("disable-model-invocation: true")
    );
    aru(&fixture.project)
        .args(["target", "add", "claude"])
        .assert()
        .success();
    fixture.check();
    assert_eq!(
        std::fs::read(fixture.project.join(".claude/skills/review/SKILL.md")).unwrap(),
        std::fs::read(&shared).unwrap()
    );
}

#[test]
fn independent_copies_do_not_share_metadata_overrides() {
    let fixture = Fixture::new(&["pi", "claude"]);
    fixture.edit();
    aru(&fixture.project).arg("sync").assert().success();
    fixture.upstream("disable-model-invocation: false\n", "New body");
    aru(&fixture.project)
        .args(["skill", "update"])
        .assert()
        .success();
    assert!(
        std::fs::read_to_string(fixture.path())
            .unwrap()
            .contains("disable-model-invocation: true")
    );
    let claude = fixture.project.join(".claude/skills/review/SKILL.md");
    assert!(
        std::fs::read_to_string(claude)
            .unwrap()
            .contains("disable-model-invocation: false")
    );
    fixture.check();
    #[cfg(unix)]
    {
        let before = aru::transaction::path_digest(&fixture.project).unwrap();
        aru(&fixture.project)
            .args(["target", "add", "codex"])
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "different local metadata overrides",
            ));
        assert_eq!(
            aru::transaction::path_digest(&fixture.project).unwrap(),
            before
        );
    }
}
