#![cfg(unix)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use expectrl::{ControlCode, Eof, Expect, Session};
use predicates::prelude::*;

fn aru(root: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.current_dir(root).arg("--offline");
    command
}

fn terminal(root: &Path, args: &[&str]) -> expectrl::session::OsSession {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
    command.current_dir(root).arg("--offline").args(args);
    let mut session = Session::spawn(command).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(20)));
    session
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(|entry| {
            let entry = entry.unwrap();
            let bytes = entry
                .file_type()
                .is_file()
                .then(|| std::fs::read(entry.path()).unwrap());
            (
                entry.path().strip_prefix(root).unwrap().to_path_buf(),
                bytes,
            )
        })
        .collect()
}

fn init(root: &Path) {
    aru(root)
        .args(["init", "--target", "codex", "--target", "claude"])
        .assert()
        .success();
}

fn git(root: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap()
            .status
            .success()
    );
}

fn repository(root: &Path) {
    repository_named(root, "demo");
}

fn repository_named(root: &Path, name: &str) {
    let skill = root.join("skills").join(name);
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Demo\n---\n# Demo\n"),
    )
    .unwrap();
    git(root, &["init", "--quiet"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "user.email", "test@example.com"]);
    git(root, &["add", "skills"]);
    git(root, &["commit", "--quiet", "-m", "initial"]);
    git(root, &["tag", "1.0.0"]);
}

#[test]
fn bare_init_and_target_lifecycle_use_filtered_multiselects() {
    let root = tempfile::tempdir().unwrap();
    let mut session = terminal(root.path(), &["init"]);
    session.expect("Select project targets").unwrap();
    session.send("codex \r").unwrap();
    session.expect("Initialized aru project for codex").unwrap();
    session.expect(Eof).unwrap();

    for (action, prompt) in [
        ("add", "Select targets to add"),
        ("remove", "Select targets to remove"),
    ] {
        let mut session = terminal(root.path(), &["target", action]);
        session.expect(prompt).unwrap();
        session.send("claude \r").unwrap();
        session.expect("Targets synchronized").unwrap();
        session.expect(Eof).unwrap();
        let manifest = aru::manifest::ManifestDocument::load(root.path())
            .unwrap()
            .manifest()
            .unwrap();
        assert_eq!(
            manifest
                .project
                .targets
                .contains(&aru::manifest::Target::Claude),
            action == "add"
        );
    }
    let mut session = terminal(root.path(), &["target", "set"]);
    session.expect("Select project targets").unwrap();
    session.send("\x1b[D").unwrap(); // clear defaults
    session.send("claude \r").unwrap();
    session.expect("Targets synchronized: claude").unwrap();
    session.expect(Eof).unwrap();
    aru(root.path())
        .args(["sync", "--locked", "--dry-run"])
        .assert()
        .success();
}

#[test]
fn scope_target_and_skill_prompts_install_project_or_global() {
    for global in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let home = temp.path().join("home");
        let source = temp.path().join("source");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&home).unwrap();
        repository(&source);
        // Choosing global must bypass even invalid nearby project state.
        if global {
            std::fs::write(project.join("aru.toml"), "invalid [").unwrap();
        }
        let before = snapshot(&project);
        let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
        command
            .current_dir(&project)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env_remove("CODEX_HOME")
            .args(["--offline", "skill", "add", source.to_str().unwrap()]);
        let mut session = Session::spawn(command).unwrap();
        session.set_expect_timeout(Some(Duration::from_secs(20)));
        session.expect("Installation scope").unwrap();
        if global {
            session.send("\x1b[B").unwrap();
        }
        session.send("\r").unwrap();
        session.expect("Select targets to install to").unwrap();
        session.send("codex \r").unwrap();
        session.expect("Select skills to install").unwrap();
        session.send(" \r").unwrap();
        session
            .expect(if global {
                "Global skills installed"
            } else {
                "Standalone skills installed"
            })
            .unwrap();
        session.expect(Eof).unwrap();
        if global {
            assert_eq!(snapshot(&project), before);
            assert!(home.join(".codex/skills/demo/SKILL.md").is_file());
        } else {
            assert!(project.join(".agents/skills/demo/SKILL.md").is_file());
            assert_eq!(std::fs::read_dir(&home).unwrap().count(), 0);
        }
        assert!(!project.join("aru.lock").exists());
    }
}

#[test]
fn managed_skill_prompt_limits_installation_to_selected_target() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    repository(&source);
    init(&project);
    let mut session = terminal(&project, &["skill", "add", source.to_str().unwrap()]);
    session.expect("Installation scope").unwrap();
    session.send("\r").unwrap();
    session.expect("Select dependency targets").unwrap();
    session.send("\x1b[D").unwrap();
    session.send("claude \r").unwrap();
    session.expect("Select skills to install").unwrap();
    session.send(" \r").unwrap();
    session.expect(Eof).unwrap();
    assert!(project.join(".claude/skills/demo/SKILL.md").is_file());
    assert!(!project.join(".agents/skills/demo").exists());
    aru(&project)
        .args(["sync", "--locked", "--dry-run"])
        .assert()
        .success();
}

#[test]
fn instruction_path_entry_and_removal_keep_canonical_content() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    std::fs::write(root.path().join("AGENTS.md"), "# Keep\n").unwrap();
    let mut session = terminal(root.path(), &["instruction", "add"]);
    session.expect("Project-relative AGENTS.md path").unwrap();
    session.send("AGENTS.md\r").unwrap();
    session.expect(Eof).unwrap();
    assert!(root.path().join("CLAUDE.md").exists());
    let mut session = terminal(root.path(), &["instruction", "remove"]);
    session
        .expect("Select instruction selectors to remove")
        .unwrap();
    session.send(" \r").unwrap();
    session.expect(Eof).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.path().join("AGENTS.md")).unwrap(),
        "# Keep\n"
    );
    assert!(!root.path().join("CLAUDE.md").exists());
}

#[test]
fn mcp_add_targets_and_remove_name_are_guided() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    let mut session = terminal(
        root.path(),
        &[
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
        ],
    );
    session.expect("Select dependency targets").unwrap();
    session.send("\x1b[D").unwrap();
    session.send("claude \r").unwrap();
    session.expect(Eof).unwrap();
    assert!(root.path().join(".mcp.json").is_file());
    assert!(!root.path().join(".codex/config.toml").exists());
    let mut session = terminal(root.path(), &["mcp", "remove"]);
    session.expect("Select MCP server to remove").unwrap();
    session.send("\r").unwrap();
    session.expect(Eof).unwrap();
    let manifest = aru::manifest::ManifestDocument::load(root.path())
        .unwrap()
        .manifest()
        .unwrap();
    assert!(manifest.mcp.is_empty());
}

#[test]
fn configured_resource_menus_cancel_without_writes_or_fetching() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("aru.toml"),
        r#"
[project]
targets = ["codex", "claude"]
[packages]
"example/package" = { version = "*" }
[skills]
"example/skills" = { version = "*", include = ["*"] }
[plugins.tools]
source = "example/plugin"
format = "agent-plugins"
version = "*"
[mcp.docs]
server = "example/docs"
"#,
    )
    .unwrap();
    aru::manifest::ManifestDocument::load(root.path())
        .unwrap()
        .manifest()
        .unwrap();
    let before = snapshot(root.path());
    for (args, message) in [
        (vec!["remove"], "Select package to remove"),
        (vec!["update"], "Select packages to update"),
        (vec!["skill", "remove"], "Select skill source to remove"),
        (vec!["skill", "update"], "Select skill sources to update"),
        (vec!["plugin", "remove"], "Select plugin to remove"),
        (vec!["plugin", "update"], "Select plugins to update"),
        (vec!["mcp", "remove"], "Select MCP server to remove"),
        (vec!["mcp", "update"], "Select MCP servers to update"),
        (
            vec!["add", "example/new-package"],
            "Select dependency targets",
        ),
        (
            vec!["plugin", "add", "example/new-plugin"],
            "Select dependency targets",
        ),
        (vec!["target", "add"], "Select targets to add"),
        (vec!["target", "remove"], "Select targets to remove"),
        (vec!["target", "set"], "Select project targets"),
        (
            vec!["instruction", "add"],
            "Project-relative AGENTS.md path",
        ),
    ] {
        let mut session = terminal(root.path(), &args);
        session.expect(message).unwrap();
        session.send(ControlCode::ESC).unwrap();
        session.expect("Selection canceled").unwrap();
        session.expect(Eof).unwrap();
        assert_eq!(snapshot(root.path()), before, "{args:?}");
    }
}

#[test]
fn scope_and_init_cancel_without_creating_state() {
    for args in [vec!["init"], vec!["skill", "add", "example/missing"]] {
        let root = tempfile::tempdir().unwrap();
        let mut session = terminal(root.path(), &args);
        session
            .expect(if args[0] == "init" {
                "Select project targets"
            } else {
                "Installation scope"
            })
            .unwrap();
        session.send(ControlCode::ESC).unwrap();
        session.expect("Selection canceled").unwrap();
        session.expect(Eof).unwrap();
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    }
}

#[test]
fn target_dry_run_and_concurrent_manifest_change_do_not_apply_selections() {
    for (dry_run, concurrent) in [(true, false), (true, true), (false, true)] {
        let root = tempfile::tempdir().unwrap();
        init(root.path());
        let before = snapshot(root.path());
        let mut args = vec!["target", "add"];
        if dry_run {
            args.push("--dry-run");
        }
        let mut session = terminal(root.path(), &args);
        session.expect("Select targets to add").unwrap();
        if concurrent {
            let path = root.path().join("aru.toml");
            let mut bytes = std::fs::read_to_string(&path).unwrap();
            bytes.push_str("\n# concurrent edit\n");
            std::fs::write(path, bytes).unwrap();
        }
        let expected = snapshot(root.path());
        session.send("pi \r").unwrap();
        session
            .expect(if concurrent {
                "changed during interactive selection"
            } else {
                "Dry run complete"
            })
            .unwrap();
        session.expect(Eof).unwrap();
        assert_eq!(snapshot(root.path()), expected);
        if !concurrent {
            assert_eq!(snapshot(root.path()), before);
        }
    }
}

#[test]
fn no_interactive_disables_prompts_even_in_a_terminal() {
    let root = tempfile::tempdir().unwrap();
    let mut session = terminal(root.path(), &["init", "--no-interactive"]);
    session.expect("pass --target").unwrap();
    session.expect(Eof).unwrap();
    let mut session = terminal(
        root.path(),
        &[
            "skill",
            "add",
            "example/missing",
            "--all",
            "--no-interactive",
        ],
    );
    session.expect("pass --target").unwrap();
    session.expect(Eof).unwrap();
    let mut session = terminal(
        root.path(),
        &[
            "mcp",
            "add",
            "--url",
            "https://example.com/mcp",
            "--name",
            "docs",
            "--no-interactive",
        ],
    );
    session.expect("pass --target").unwrap();
    session.expect(Eof).unwrap();
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn native_package_plugin_and_skill_lifecycles_accept_interactive_choices() {
    for kind in ["package", "plugin", "skill"] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        repository(&source);
        if kind == "package" {
            std::fs::write(
                source.join("aru.toml"),
                "[package]\nname = 'kit'\nversion = '1.1.0'\n[skills]\ndemo = 'skills/demo'\n",
            )
            .unwrap();
        } else if kind == "plugin" {
            std::fs::write(source.join("plugin.json"), r#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"kit","version":"1.1.0"}"#).unwrap();
        } else {
            std::fs::write(source.join("README.md"), "Skill source\n").unwrap();
        }
        // Stage only the fixture files present for this source kind.
        git(
            &source,
            &[
                "add",
                if kind == "package" {
                    "aru.toml"
                } else if kind == "plugin" {
                    "plugin.json"
                } else {
                    "README.md"
                },
            ],
        );
        git(&source, &["commit", "--quiet", "-m", "metadata"]);
        git(&source, &["tag", "1.1.0"]);
        init(&project);
        let mut add = if kind == "package" {
            vec!["add"]
        } else {
            vec![kind, "add"]
        };
        add.push(source.to_str().unwrap());
        if kind == "skill" {
            add.extend(["--scope", "project", "--all"]);
        }
        if kind == "plugin" {
            add.extend(["--component", "skills"]);
        }
        let mut session = terminal(&project, &add);
        session.expect("Select dependency targets").unwrap();
        session.send("\r").unwrap();
        session.expect(Eof).unwrap();
        assert!(
            project.join(".agents/skills/demo/SKILL.md").is_file(),
            "{kind}"
        );
        assert!(
            project.join(".claude/skills/demo/SKILL.md").is_file(),
            "{kind}"
        );

        let update = if kind == "package" {
            vec!["update"]
        } else {
            vec![kind, "update"]
        };
        let mut session = terminal(&project, &update);
        session
            .expect(match kind {
                "package" => "Select packages to update",
                "plugin" => "Select plugins to update",
                _ => "Select skill sources to update",
            })
            .unwrap();
        session.send("\r").unwrap();
        let output = session.expect(Eof).unwrap();
        let output = String::from_utf8_lossy(output.as_bytes());
        assert!(
            output.contains("Project is synchronized."),
            "{kind}: {output}"
        );
        aru(&project)
            .args(["sync", "--locked", "--dry-run"])
            .assert()
            .success();

        let remove = if kind == "package" {
            vec!["remove"]
        } else {
            vec![kind, "remove"]
        };
        let mut session = terminal(&project, &remove);
        session
            .expect(match kind {
                "package" => "Select package to remove",
                "plugin" => "Select plugin to remove",
                _ => "Select skill source to remove",
            })
            .unwrap();
        session.send("\r").unwrap();
        session.expect(Eof).unwrap();
        assert!(!project.join(".agents/skills/demo").exists(), "{kind}");
        assert!(!project.join(".claude/skills/demo").exists(), "{kind}");
        let manifest = aru::manifest::ManifestDocument::load(&project)
            .unwrap()
            .manifest()
            .unwrap();
        assert!(
            manifest.packages.is_empty()
                && manifest.plugins.is_empty()
                && manifest.skills.is_empty()
        );
    }
}

#[test]
fn update_multiselect_refreshes_only_the_selected_skill_source() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    init(&project);
    for (name, target) in [("alpha", "codex"), ("beta", "claude")] {
        let source = temp.path().join(name);
        repository_named(&source, name);
        aru(&project)
            .args([
                "skill",
                "add",
                source.to_str().unwrap(),
                "--all",
                "--target",
                target,
            ])
            .assert()
            .success();
        std::fs::write(source.join("README.md"), "New release\n").unwrap();
        git(&source, &["add", "README.md"]);
        git(&source, &["commit", "--quiet", "-m", "release"]);
        git(&source, &["tag", "1.1.0"]);
    }
    let mut session = terminal(&project, &["skill", "update"]);
    session.expect("Select skill sources to update").unwrap();
    session.send("\x1b[D").unwrap();
    session.send("alpha \r").unwrap();
    session.expect(Eof).unwrap();
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    for package in lock.skill_packages {
        assert_eq!(
            package.version,
            if package.source.contains("alpha") {
                "1.1.0"
            } else {
                "1.0.0"
            }
        );
    }
    // The escape hatch preserves update-all semantics in a real terminal.
    let mut session = terminal(&project, &["skill", "update", "--no-interactive"]);
    session.expect(Eof).unwrap();
    let lock = aru::lockfile::Lockfile::load_optional(&project)
        .unwrap()
        .unwrap();
    assert!(
        lock.skill_packages
            .iter()
            .all(|package| package.version == "1.1.0")
    );
}

#[test]
fn skill_menu_cancel_leaves_managed_project_and_cache_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let source = temp.path().join("source");
    std::fs::create_dir(&project).unwrap();
    init(&project);
    repository(&source);
    let before = snapshot(&project);
    let mut session = terminal(
        &project,
        &[
            "skill",
            "add",
            source.to_str().unwrap(),
            "--scope",
            "project",
        ],
    );
    session.expect("Select dependency targets").unwrap();
    session.send("\r").unwrap();
    session.expect("Select skills to install").unwrap();
    session.send(ControlCode::ESC).unwrap();
    session.expect("Skill selection canceled").unwrap();
    session.expect(Eof).unwrap();
    assert_eq!(snapshot(&project), before);
}

#[test]
fn selection_interrupt_and_last_target_removal_preserve_project_state() {
    for interrupt in [true, false] {
        let root = tempfile::tempdir().unwrap();
        aru(root.path())
            .args(["init", "--target", "codex"])
            .assert()
            .success();
        let before = snapshot(root.path());
        let mut session = terminal(root.path(), &["target", "remove"]);
        session.expect("Select targets to remove").unwrap();
        if interrupt {
            session.send(ControlCode::ETX).unwrap();
            session
                .expect("interactive selection was interrupted")
                .unwrap();
        } else {
            session.send(" \r").unwrap();
            session.expect("cannot remove the last target").unwrap();
        }
        session.expect(Eof).unwrap();
        assert_eq!(snapshot(root.path()), before);
    }
}

#[test]
fn missing_required_selections_fail_without_a_terminal() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["init"],
        vec!["remove"],
        vec!["instruction", "add"],
        vec!["instruction", "remove"],
        vec!["target", "add"],
        vec!["target", "remove"],
        vec!["target", "set"],
        vec!["skill", "remove"],
        vec!["mcp", "remove"],
        vec!["plugin", "remove"],
    ] {
        aru(root.path())
            .args(args)
            .assert()
            .failure()
            .stderr(predicate::str::contains(
                "interactive selection requires a terminal",
            ));
    }
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}
