use super::*;

#[test]
fn projection_flags_preserve_no_sync_precedence() {
    for merge in [false, true] {
        for force in [false, true] {
            assert!(matches!(
                package_projection(true, merge, force).unwrap(),
                ProjectionPolicy::LockOnly
            ));
        }
    }
    for (merge, force, expected) in [
        (false, false, CollisionPolicy::Reject),
        (true, false, CollisionPolicy::MergeInstructions),
        (false, true, CollisionPolicy::Force),
    ] {
        assert!(
            matches!(package_projection(false, merge, force).unwrap(), ProjectionPolicy::Project(actual) if actual == expected)
        );
    }
    assert_eq!(
        package_projection(false, true, true)
            .unwrap_err()
            .to_string(),
        "--merge and --force cannot be combined"
    );
}

#[test]
fn declared_package_and_trust_keys_preserve_their_distinct_lookup_order() {
    let temporary = tempfile::tempdir().unwrap();
    let project = temporary.path();
    std::fs::create_dir(project.join("repo")).unwrap();
    let mut manifest = ManifestDocument::new(&[crate::manifest::Target::Codex])
        .manifest()
        .unwrap();
    for key in ["./repo/.", "repo"] {
        manifest.packages.insert(key.into(), Default::default());
        manifest
            .package_trust
            .insert(key.into(), Default::default());
    }
    assert_eq!(
        find_package_key(project, &manifest, "repo")
            .unwrap()
            .as_deref(),
        Some("repo")
    );
    assert_eq!(
        find_trust_key(project, &manifest, "repo")
            .unwrap()
            .as_deref(),
        Some("./repo/.")
    );
    let absolute = project.join("repo");
    assert_eq!(
        find_package_key(project, &manifest, absolute.to_str().unwrap())
            .unwrap()
            .as_deref(),
        Some("./repo/.")
    );
    manifest
        .packages
        .insert("owner/repo".into(), Default::default());
    assert_eq!(
        find_package_key(project, &manifest, "https://github.com/owner/repo.git")
            .unwrap()
            .as_deref(),
        Some("owner/repo")
    );
    assert_eq!(
        find_package_key(project, &manifest, "other/repo").unwrap(),
        None
    );

    // Exact package keys bypass canonicalization; trust never does, even when
    // the declaration text matches. Preserve errors for missing local sources.
    manifest
        .packages
        .insert("./missing/.".into(), Default::default());
    manifest
        .package_trust
        .insert("./missing/.".into(), Default::default());
    assert_eq!(
        find_package_key(project, &manifest, "./missing/.")
            .unwrap()
            .as_deref(),
        Some("./missing/.")
    );
    assert!(find_trust_key(project, &manifest, "./missing/.").is_err());
    assert!(find_trust_key(project, &manifest, "./repo/.").is_err());
}
