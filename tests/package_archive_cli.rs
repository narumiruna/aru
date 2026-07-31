use std::path::Path;
use std::process::Command;

use assert_cmd::cargo::cargo_bin_cmd;
use flate2::read::GzDecoder;
use predicates::prelude::*;
use tar::Archive;

fn aru(package: &Path) -> assert_cmd::Command {
    let mut command = cargo_bin_cmd!("aru");
    command.args(["--project", package.to_str().unwrap()]);
    command
}

fn git(repository: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success(), "git {arguments:?} failed");
}

fn copy_tree(source: &Path, destination: &Path) {
    for entry in walkdir::WalkDir::new(source) {
        let entry = entry.unwrap();
        let relative = entry.path().strip_prefix(source).unwrap();
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn package_repository(root: &Path) {
    std::fs::create_dir(root).unwrap();
    copy_tree(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/packages/valid")
            .as_path(),
        root,
    );
    git(root, &["init", "--quiet"]);
    git(root, &["config", "user.email", "archive@example.com"]);
    git(root, &["config", "user.name", "archive tests"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "initial"]);
}

#[test]
fn package_list_and_archive_are_deterministic_with_normalized_headers() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("package");
    package_repository(&repository);

    aru(&repository)
        .args(["package", "--list"])
        .assert()
        .success()
        .stdout(concat!(
            "AGENTS.md\n",
            "aru.toml\n",
            "skills/review/SKILL.md\n",
        ));
    assert!(!repository.join("target/aru-package").exists());

    aru(&repository)
        .arg("package")
        .assert()
        .success()
        .stdout("")
        .stderr(predicate::str::contains("Packaged agent-kit v1.2.0"));
    let archive_path = repository.join("target/aru-package/agent-kit-1.2.0.aru-package.tar.gz");
    assert!(!repository.join(".aru").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&archive_path)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
    let first = std::fs::read(&archive_path).unwrap();
    assert_eq!(
        first.as_slice(),
        include_bytes!("fixtures/contracts/agent-kit-1.2.0.aru-package.tar.gz")
    );
    aru(&repository)
        .args(["--offline", "package"])
        .assert()
        .success();
    let second = std::fs::read(&archive_path).unwrap();
    assert_eq!(first, second);

    let mut archive = Archive::new(GzDecoder::new(first.as_slice()));
    let mut paths = Vec::new();
    for entry in archive.entries().unwrap() {
        let entry = entry.unwrap();
        let header = entry.header();
        assert_eq!(header.mtime().unwrap(), 0);
        assert_eq!(header.uid().unwrap(), 0);
        assert_eq!(header.gid().unwrap(), 0);
        assert!(matches!(header.mode().unwrap(), 0o644 | 0o755));
        paths.push(entry.path().unwrap().to_string_lossy().into_owned());
    }
    assert_eq!(paths, ["AGENTS.md", "aru.toml", "skills/review/SKILL.md"]);
}

#[cfg(unix)]
#[test]
fn package_archive_preserves_only_the_git_executable_bit() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("package");
    let output = temporary.path().join("executable.tar.gz");
    package_repository(&repository);
    std::fs::create_dir(repository.join("bin")).unwrap();
    std::fs::write(repository.join("bin/tool"), "#!/bin/sh\necho safe\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(
        repository.join("bin/tool"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    git(&repository, &["add", "bin/tool"]);
    git(&repository, &["commit", "--quiet", "-m", "executable"]);
    aru(&repository)
        .args(["package", "--output", output.to_str().unwrap()])
        .assert()
        .success();
    let bytes = std::fs::read(output).unwrap();
    let mut archive = Archive::new(GzDecoder::new(bytes.as_slice()));
    let mode = archive
        .entries()
        .unwrap()
        .map(std::result::Result::unwrap)
        .find(|entry| entry.path().unwrap() == Path::new("bin/tool"))
        .unwrap()
        .header()
        .mode()
        .unwrap();
    assert_eq!(mode, 0o755);
}

#[test]
fn dirty_package_requires_explicit_acknowledgement_and_output_can_be_selected() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("package");
    let output = temporary.path().join("selected.tar.gz");
    package_repository(&repository);
    std::fs::write(repository.join("AGENTS.md"), "dirty package rules\n").unwrap();

    aru(&repository)
        .arg("package")
        .assert()
        .failure()
        .stderr(predicate::str::contains("working tree is dirty"));
    assert!(!repository.join("target/aru-package").exists());

    aru(&repository)
        .args([
            "package",
            "--allow-dirty",
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("dirty Git worktree"))
        .stderr(predicate::str::contains("Packaged agent-kit"));
    assert!(output.is_file());
}

#[test]
fn package_validates_dependency_graph_before_writing_archive() {
    let temporary = tempfile::tempdir().unwrap();
    let dependency = temporary.path().join("dependency");
    let repository = temporary.path().join("package");
    package_repository(&dependency);
    package_repository(&repository);
    let mut manifest = std::fs::read_to_string(repository.join("aru.toml")).unwrap();
    manifest.push_str(&format!(
        "\n[dependencies.\"{}\"]\nversion = \"*\"\n",
        dependency.display()
    ));
    std::fs::write(repository.join("aru.toml"), manifest).unwrap();

    aru(&repository)
        .args(["package", "--allow-dirty"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("transitive local package"));
    assert!(!repository.join("target/aru-package").exists());
}

#[cfg(unix)]
#[test]
fn package_rejects_symlinks_case_collisions_and_hidden_unicode() {
    let temporary = tempfile::tempdir().unwrap();

    let symlink = temporary.path().join("symlink");
    package_repository(&symlink);
    aru(&symlink).arg("package").assert().success();
    let archive = symlink.join("target/aru-package/agent-kit-1.2.0.aru-package.tar.gz");
    let original_archive = std::fs::read(&archive).unwrap();
    std::os::unix::fs::symlink("AGENTS.md", symlink.join("rules-link")).unwrap();
    aru(&symlink)
        .args(["package", "--allow-dirty"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("regular file"));
    assert_eq!(std::fs::read(&archive).unwrap(), original_archive);

    let collision = temporary.path().join("collision");
    package_repository(&collision);
    std::fs::write(collision.join("Rules.md"), "upper\n").unwrap();
    std::fs::write(collision.join("rules.md"), "lower\n").unwrap();
    aru(&collision)
        .args(["package", "--allow-dirty"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("case-insensitive"));

    let escaped = temporary.path().join("escaped");
    let outside = temporary.path().join("outside");
    package_repository(&escaped);
    std::fs::write(escaped.join(".gitignore"), "target\n").unwrap();
    git(&escaped, &["add", ".gitignore"]);
    git(&escaped, &["commit", "--quiet", "-m", "ignore target"]);
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, escaped.join("target")).unwrap();
    aru(&escaped)
        .arg("package")
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a real directory"));
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());

    let hidden = temporary.path().join("hidden");
    package_repository(&hidden);
    std::fs::write(hidden.join("hidden.md"), "hidden \u{202e}text\n").unwrap();
    aru(&hidden)
        .args(["package", "--allow-dirty"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("hidden Unicode U+202E"));
}
