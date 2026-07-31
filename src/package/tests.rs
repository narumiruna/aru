use super::*;

#[test]
fn valid_fixture_parses_and_tree_is_bounded() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/packages/valid");
    let manifest = PackageManifest::load(&root).unwrap();
    assert_eq!(manifest.package.name, "agent-kit");
    assert_eq!(manifest.package.version, "1.2.0");
    assert_eq!(manifest.skills["review"], "skills/review");
    assert_eq!(manifest.instructions.sources.len(), 1);
    let mut budget = TreeBudget::default();
    validate_tree(&root, &mut budget).unwrap();
    assert!(budget.entries >= 4);
    assert!(budget.bytes > 0);
}

#[test]
fn legacy_package_manifest_is_not_loaded() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("aru-package.toml"),
        "[package]\nname='demo'\nversion='1.0.0'\n",
    )
    .unwrap();

    let error = PackageManifest::load(root.path()).unwrap_err();
    assert!(error.to_string().contains("no aru.toml"));
}

#[test]
fn unknown_nested_fields_and_invalid_versions_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    for (name, text, expected) in [
        (
            "unknown-root",
            "[package]\nname='demo'\nversion='1.0.0'\nfuture=true\n",
            "package.future",
        ),
        (
            "unknown-mcp",
            "[package]\nname='demo'\nversion='1.0.0'\n[mcp.docs]\nurl='https://example.com/mcp'\nsecret='value'\n",
            "mcp.docs.secret",
        ),
        (
            "invalid-version",
            "[package]\nname='demo'\nversion='latest'\n",
            "invalid aru package version",
        ),
    ] {
        let root = temporary.path().join(name);
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join(crate::manifest::MANIFEST_FILE), text).unwrap();
        assert!(
            PackageManifest::load(&root)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn package_tree_rejects_hidden_unicode_and_symlinks() {
    let hidden = tempfile::tempdir().unwrap();
    std::fs::write(
        hidden.path().join(crate::manifest::MANIFEST_FILE),
        "[package]\nname='demo'\nversion='1.0.0'\n",
    )
    .unwrap();
    std::fs::write(hidden.path().join("rules.md"), "hidden \u{202e}text\n").unwrap();
    assert!(
        validate_tree(hidden.path(), &mut TreeBudget::default())
            .unwrap_err()
            .to_string()
            .contains("hidden Unicode U+202E")
    );

    #[cfg(unix)]
    {
        let linked = tempfile::tempdir().unwrap();
        std::fs::write(
            linked.path().join(crate::manifest::MANIFEST_FILE),
            "[package]\nname='demo'\nversion='1.0.0'\n",
        )
        .unwrap();
        std::fs::write(linked.path().join("target"), "content").unwrap();
        std::os::unix::fs::symlink("target", linked.path().join("link")).unwrap();
        assert!(
            validate_tree(linked.path(), &mut TreeBudget::default())
                .unwrap_err()
                .to_string()
                .contains("regular file or directory")
        );
    }
}
