#![cfg(unix)]

use assert_cmd::cargo::cargo_bin_cmd;

#[test]
fn missing_legacy_temp_root_does_not_block_init_or_previews() {
    let root = tempfile::tempdir().unwrap();
    let managed = root.path().join("managed");
    let standalone = root.path().join("standalone");
    let missing = root.path().join("missing/temp");
    std::fs::create_dir(&managed).unwrap();
    std::fs::create_dir(&standalone).unwrap();
    cargo_bin_cmd!("aru")
        .current_dir(&managed)
        .env("TMPDIR", &missing)
        .args(["init", "--target", "codex"])
        .assert()
        .success();
    cargo_bin_cmd!("aru")
        .current_dir(&managed)
        .env("TMPDIR", &missing)
        .args(["sync"])
        .assert()
        .success();
    cargo_bin_cmd!("aru")
        .current_dir(&standalone)
        .env("TMPDIR", &missing)
        .args([
            "mcp",
            "add",
            "--target",
            "claude",
            "--url",
            "https://example.com/mcp",
            "--name",
            "demo",
            "--dry-run",
        ])
        .assert()
        .success();
    assert!(!missing.exists());
    assert_eq!(std::fs::read_dir(&standalone).unwrap().count(), 0);
}

#[test]
fn non_directory_legacy_entries_do_not_block_managed_work_or_preview() {
    for symlink in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        let temporary = root.path().join("temporary");
        std::fs::create_dir(&project).unwrap();
        let canonical = project.canonicalize().unwrap();
        let digest = aru::digest::sha256_bytes(canonical.as_os_str().as_encoded_bytes());
        let control = temporary
            .join("aru-standalone")
            .join(digest.strip_prefix("sha256:").unwrap());
        std::fs::create_dir_all(control.parent().unwrap()).unwrap();
        if symlink {
            std::os::unix::fs::symlink(&project, &control).unwrap();
        } else {
            std::fs::write(&control, "unrelated").unwrap();
        }
        cargo_bin_cmd!("aru")
            .current_dir(&project)
            .env("TMPDIR", &temporary)
            .args([
                "mcp",
                "add",
                "--target",
                "claude",
                "--url",
                "https://example.com/mcp",
                "--name",
                "demo",
                "--dry-run",
            ])
            .assert()
            .success();
        cargo_bin_cmd!("aru")
            .current_dir(&project)
            .env("TMPDIR", &temporary)
            .args(["init", "--target", "codex"])
            .assert()
            .success();
        cargo_bin_cmd!("aru")
            .current_dir(&project)
            .env("TMPDIR", &temporary)
            .args(["sync", "--dry-run"])
            .assert()
            .success();
        if symlink {
            assert_eq!(std::fs::read_link(&control).unwrap(), project);
        } else {
            assert_eq!(std::fs::read(&control).unwrap(), b"unrelated");
        }
    }
}
