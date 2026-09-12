#![cfg(unix)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use assert_cmd::cargo::cargo_bin_cmd;
use expectrl::{Eof, Expect, Session};
use predicates::prelude::*;

fn aru(root: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.current_dir(root).arg("--offline");
    command
}

fn terminal(root: &Path, args: &[&str], term: &str) -> expectrl::session::OsSession {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aru"));
    command
        .current_dir(root)
        .env("TERM", term)
        .args(["--offline", "--color", "never"])
        .args(args);
    let mut session = Session::spawn(command).unwrap();
    session.set_expect_timeout(Some(Duration::from_secs(20)));
    session
}

fn terminal_output(root: &Path, args: &[&str], term: &str) -> String {
    let output = terminal(root, args, term).expect(Eof).unwrap();
    String::from_utf8_lossy(output.as_bytes()).into_owned()
}

fn mcp_args() -> Vec<&'static str> {
    vec![
        "mcp",
        "add",
        "--url",
        "https://example.com/mcp",
        "--name",
        "docs",
        "--target",
        "codex",
    ]
}

fn has_sgr(text: &str) -> bool {
    text.split("\x1b[")
        .skip(1)
        .any(|suffix| suffix.chars().find(|ch| !ch.is_ascii_digit() && *ch != ';') == Some('m'))
}

#[test]
fn terminal_init_shows_step_help_capabilities_and_pre_apply_summary_without_color() {
    let root = tempfile::tempdir().unwrap();
    let mut session = terminal(root.path(), &["init", "--no-progress"], "xterm-256color");
    let before = session.expect("Select project targets").unwrap();
    session.send("codex").unwrap();
    let options = session.expect("instructions, skills, MCP").unwrap();
    session.send(" \r").unwrap();
    let after = session.expect(Eof).unwrap();
    let text = [before.as_bytes(), options.as_bytes(), after.as_bytes()].concat();
    let text = String::from_utf8_lossy(&text);
    assert!(text.contains("Choose project targets"), "{text}");
    assert!(text.contains("space select"), "{text}");
    assert!(text.contains("planned actions"), "{text}");
    assert!(
        text.find("create aru.toml (targets: codex)").unwrap() < text.find("Initialized").unwrap()
    );
    assert!(!has_sgr(&text), "{text}");
    assert!(root.path().join("aru.toml").is_file());
}

#[test]
fn quiet_preserves_required_prompts_but_hides_steps_and_summaries() {
    let root = tempfile::tempdir().unwrap();
    let mut session = terminal(root.path(), &["init", "--quiet"], "xterm-256color");
    let before = session.expect("Select project targets").unwrap();
    session.send("codex \r").unwrap();
    let after = session.expect(Eof).unwrap();
    let text = [before.as_bytes(), after.as_bytes()].concat();
    let text = String::from_utf8_lossy(&text);
    assert!(!text.contains("Step"), "{text}");
    assert!(!text.contains("planned actions"), "{text}");
    assert!(!text.contains("Finished"), "{text}");
    assert!(!has_sgr(&text), "{text}");
    assert!(root.path().join("aru.toml").is_file());
}

#[test]
fn terminal_installs_show_plans_before_results_and_scope_progress_to_work() {
    for managed in [false, true] {
        for no_progress in [false, true] {
            let root = tempfile::tempdir().unwrap();
            if managed {
                aru(root.path())
                    .args(["init", "--target", "codex"])
                    .assert()
                    .success();
            }
            let mut args = mcp_args();
            if no_progress {
                args.push("--no-progress");
            }
            let text = terminal_output(root.path(), &args, "xterm-256color");
            assert!(text.contains("planned actions"), "{text}");
            assert!(
                text.find("create MCP docs").unwrap() < text.find("Created").unwrap(),
                "{text}"
            );
            assert_eq!(text.contains("Resolving"), !no_progress, "{text}");
            assert_eq!(text.contains("Applying"), !no_progress, "{text}");
            assert!(!has_sgr(&text), "{text}");
            if no_progress {
                assert!(!text.contains('\x1b'), "{text}");
            } else {
                let completed = text.find("Created").unwrap();
                assert!(text[..completed].contains('\x1b'), "{text}");
                assert!(!text[completed..].contains('\x1b'), "{text}");
            }
            assert!(root.path().join(".codex/config.toml").is_file());
        }
    }
}

#[test]
fn quiet_no_interactive_and_dumb_terminals_keep_static_output() {
    for flag in ["--quiet", "--no-interactive", "--no-progress"] {
        let root = tempfile::tempdir().unwrap();
        let mut args = mcp_args();
        args.push(flag);
        let text = terminal_output(root.path(), &args, "xterm-256color");
        assert!(!text.contains('\x1b'), "{flag}: {text}");
        if flag == "--quiet" {
            assert!(text.is_empty(), "{text}");
        } else if flag == "--no-interactive" {
            assert!(text.contains("Resolving"), "{text}");
            assert!(!text.contains("planned actions"), "{text}");
            assert!(!text.contains("Applying"), "{text}");
        } else {
            assert!(text.contains("planned actions"), "{text}");
            assert!(!text.contains("Resolving"), "{text}");
        }
        assert!(root.path().join(".codex/config.toml").is_file());
    }
    let root = tempfile::tempdir().unwrap();
    let text = terminal_output(root.path(), &mcp_args(), "dumb");
    assert!(text.contains("Resolving"), "{text}");
    assert!(!text.contains('\x1b'), "{text}");
}

#[test]
fn dry_run_keeps_a_full_preview_without_applying_or_writing() {
    let root = tempfile::tempdir().unwrap();
    let mut args = mcp_args();
    args.push("--dry-run");
    let text = terminal_output(root.path(), &args, "xterm-256color");
    assert!(text.contains("Preview only"), "{text}");
    assert!(
        text.contains("create MCP docs (.codex/config.toml)"),
        "{text}"
    );
    assert!(!text.contains("applying next"), "{text}");
    assert!(!text.contains("Applying"), "{text}");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn resolution_errors_clear_progress_before_the_error_and_do_not_write() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("missing");
    let text = terminal_output(
        root.path(),
        &[
            "skill",
            "add",
            source.to_str().unwrap(),
            "--scope",
            "project",
            "--all",
            "--target",
            "codex",
        ],
        "xterm-256color",
    );
    let error = text.find("error:").unwrap();
    assert!(text[..error].contains("Resolving"), "{text}");
    assert!(text[..error].contains('\x1b'), "{text}");
    assert!(!text[error..].contains('\x1b'), "{text}");
    assert!(!text.contains("planned actions"), "{text}");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn redirected_commands_keep_stdout_clean_and_do_not_render_inline_ui() {
    let root = tempfile::tempdir().unwrap();
    aru(root.path())
        .args(["init", "--target", "codex"])
        .assert()
        .success()
        .stdout("");
    aru(root.path())
        .args(mcp_args())
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains("planned actions").not())
        .stderr(predicate::str::contains("Resolving").not())
        .stderr(predicate::str::contains('\x1b').not());
    aru(root.path())
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("codex\n")
        .stderr("");
}
