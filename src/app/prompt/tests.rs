use clap::Parser;

use super::*;
use crate::cli::Cli;

#[test]
fn explicit_scopes_normalize_without_prompts() {
    for (flags, global) in [
        (vec!["--scope", "project"], false),
        (vec!["--scope", "global"], true),
        (vec!["--global"], true),
        (Vec::new(), false),
    ] {
        let mut argv = vec![
            "aru",
            "skill",
            "add",
            "owner/repo",
            "--all",
            "--target",
            "codex",
        ];
        argv.extend(flags);
        let mut cli = Cli::try_parse_from(argv).unwrap();
        assert!(matches!(
            prepare(&mut cli.command, &mut None, false).unwrap(),
            Prepared::Ready(None)
        ));
        let Command::Skill {
            command: SkillCommand::Add(args),
        } = cli.command
        else {
            panic!()
        };
        assert_eq!(args.global, global);
    }
    for args in [
        vec!["--scope", "project", "--global"],
        vec!["--scope", "global", "--global"],
        vec!["--scope", "unknown"],
    ] {
        let mut argv = vec!["aru", "skill", "add", "owner/repo"];
        argv.extend(args);
        assert!(Cli::try_parse_from(argv).is_err());
    }
}

#[test]
fn non_interactive_update_all_and_explicit_selections_remain_unchanged() {
    for args in [
        vec!["update"],
        vec!["skill", "update"],
        vec!["plugin", "update"],
        vec!["mcp", "update"],
        vec!["remove", "owner/repo"],
        vec!["skill", "remove", "owner/repo"],
        vec!["mcp", "remove", "docs"],
        vec!["plugin", "remove", "tools"],
        vec!["target", "set", "codex"],
        vec!["instruction", "add", "AGENTS.md"],
    ] {
        let mut argv = vec!["aru"];
        argv.extend(args);
        let mut cli = Cli::try_parse_from(argv).unwrap();
        let before = format!("{:?}", cli.command);
        assert!(matches!(
            prepare(&mut cli.command, &mut None, false).unwrap(),
            Prepared::Ready(None)
        ));
        assert_eq!(format!("{:?}", cli.command), before);
    }
}

#[test]
fn snapshots_reject_modified_or_new_lockfiles_and_oversized_files() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("aru.toml"), "initial").unwrap();
    let initial = ProjectSnapshot::read(root.path()).unwrap();
    initial.verify(root.path()).unwrap();
    std::fs::write(root.path().join("aru.lock"), "new lock").unwrap();
    assert!(
        initial
            .verify(root.path())
            .unwrap_err()
            .to_string()
            .contains("changed during interactive selection")
    );
    let with_lock = ProjectSnapshot::read(root.path()).unwrap();
    std::fs::write(root.path().join("aru.toml"), "changed").unwrap();
    assert!(with_lock.verify(root.path()).is_err());
    let file = std::fs::File::create(root.path().join("aru.lock")).unwrap();
    file.set_len(16 * 1024 * 1024 + 1).unwrap();
    assert!(
        ProjectSnapshot::read(root.path())
            .unwrap_err()
            .to_string()
            .contains("at most 16 MiB")
    );
}
