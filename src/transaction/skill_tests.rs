use super::*;

fn source(root: &Path) -> (PathBuf, String, Vec<u8>, String) {
    let source = root.join("source");
    std::fs::create_dir(&source).unwrap();
    let original = "---\nname: review\ndescription: Review code\n---\nOriginal body\n";
    std::fs::write(source.join("SKILL.md"), original).unwrap();
    std::fs::write(source.join("asset"), "asset").unwrap();
    let digest = crate::skill::canonical_skill_digest(&source).unwrap();
    let merged = original
        .replace(
            "description: Review code",
            "description: Review code\ndisable-model-invocation: true",
        )
        .into_bytes();
    let projected = crate::skill::skill_digest_with_document(&source, &merged).unwrap();
    (source, digest, merged, projected)
}

#[test]
fn metadata_merge_preserves_source_and_is_rolled_back_with_other_files() {
    let project = tempfile::tempdir().unwrap();
    let (source, digest, merged, projected) = source(project.path());
    std::fs::write(project.path().join("aru.lock"), "old lock").unwrap();
    set_failure_phase("ARU_TEST_FAIL_AFTER", Some(1));
    let result = apply(
        project.path(),
        vec![
            Operation::skill_directory_with_metadata(
                ".pi/skills/review",
                &source,
                &digest,
                merged.clone(),
                &projected,
            ),
            Operation::file("aru.lock", b"new lock".to_vec()),
        ],
    );
    set_failure_phase("ARU_TEST_FAIL_AFTER", None);
    assert!(result.is_err());
    assert!(!project.path().join(".pi/skills/review").exists());
    assert_eq!(
        std::fs::read(project.path().join("aru.lock")).unwrap(),
        b"old lock"
    );
    assert_eq!(
        crate::skill::canonical_skill_digest(&source).unwrap(),
        digest
    );
    apply(
        project.path(),
        vec![Operation::skill_directory_with_metadata(
            ".pi/skills/review",
            &source,
            &digest,
            merged.clone(),
            &projected,
        )],
    )
    .unwrap();
    assert_eq!(
        std::fs::read(project.path().join(".pi/skills/review/SKILL.md")).unwrap(),
        merged
    );
    assert_eq!(
        crate::skill::canonical_skill_digest(&source).unwrap(),
        digest
    );
}

#[test]
fn metadata_merge_cannot_bypass_source_or_projection_verification() {
    for failure in ["source", "projection", "body", "name", "description"] {
        let project = tempfile::tempdir().unwrap();
        let (source, mut digest, mut merged, mut projected) = source(project.path());
        match failure {
            "source" => digest = "sha256:wrong".into(),
            "projection" => projected = "sha256:wrong".into(),
            "body" | "name" | "description" => {
                let (from, to) = match failure {
                    "body" => ("Original body", "Changed body"),
                    "name" => ("name: review", "name: other"),
                    _ => ("description: Review code", "description: Changed"),
                };
                merged = String::from_utf8(merged)
                    .unwrap()
                    .replace(from, to)
                    .into_bytes();
                projected = crate::skill::skill_digest_with_document(&source, &merged).unwrap();
            }
            _ => unreachable!(),
        }
        let result = apply(
            project.path(),
            vec![Operation::skill_directory_with_metadata(
                ".pi/skills/review",
                &source,
                &digest,
                merged,
                &projected,
            )],
        );
        assert!(result.is_err(), "accepted {failure}");
        assert!(!project.path().join(".pi").exists());
        assert!(!project.path().join(JOURNAL_FILE).exists());
    }
}
