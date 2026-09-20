use std::path::Path;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

fn aru(directory: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command
        .current_dir(directory)
        .args(["--no-interactive", "--color", "never"]);
    command
}

#[test]
fn managed_commands_choose_nearest_manifest_unless_project_is_explicit() {
    let temporary = tempfile::tempdir().unwrap();
    let outer = temporary.path();
    let inner = outer.join("inner");
    let nested = inner.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(outer.join("aru.toml"), "[project]\ntargets = ['codex']\n").unwrap();
    std::fs::write(inner.join("aru.toml"), "[project]\ntargets = ['claude']\n").unwrap();
    aru(&nested)
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("claude\n");
    aru(&nested)
        .arg("--project")
        .arg(outer)
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("codex\n");
    // An explicit directory without a manifest must not fall back to an ancestor.
    aru(&nested)
        .args(["--project", ".", "target", "list"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(format!(
            "no aru.toml in {}",
            nested.canonicalize().unwrap().display()
        )));
}

#[cfg(unix)]
#[test]
fn explicit_symlink_roots_are_resolved_before_manifest_lookup() {
    let temporary = tempfile::tempdir().unwrap();
    let real = temporary.path().join("real");
    let link = temporary.path().join("link");
    std::fs::create_dir(&real).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    std::fs::write(real.join("aru.toml"), "[project]\ntargets = ['claude']\n").unwrap();
    aru(temporary.path())
        .arg("--project")
        .arg(&link)
        .args(["target", "list"])
        .assert()
        .success()
        .stdout("claude\n");
    std::fs::remove_file(real.join("aru.toml")).unwrap();
    aru(temporary.path())
        .arg("--project")
        .arg(&link)
        .args(["target", "list"])
        .assert()
        .failure()
        .stderr(format!(
            "error: no aru.toml in {}\n",
            real.canonicalize().unwrap().display()
        ));
}

#[test]
fn missing_project_errors_remain_command_specific() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    aru(root)
        .args(["target", "list"])
        .assert()
        .failure()
        .stderr(
            "error: no aru.toml found in the current directory or its ancestors; run aru init\n",
        );
    aru(root)
        .args(["package", "--list"])
        .assert()
        .failure()
        .stderr("error: no aru.toml found in the current directory or its ancestors\n");
    for args in [["target", "list"], ["package", "--list"]] {
        aru(root)
            .args(["--project", "."])
            .args(args)
            .assert()
            .failure()
            .stderr(format!(
                "error: no aru.toml in {}\n",
                root.canonicalize().unwrap().display()
            ));
    }
    let file = root.join("file");
    std::fs::write(&file, "not a directory").unwrap();
    aru(root)
        .arg("--project")
        .arg(&file)
        .args(["skill", "add", "owner/repo", "--target", "claude", "--all"])
        .assert()
        .failure()
        .stderr("error: installation root is not a directory\n");
}
