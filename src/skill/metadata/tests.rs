use super::*;
use crate::skill::canonical_skill_digest;

fn document(extra: &str, body: &str) -> Document {
    Document::parse(&format!(
        "---\nname: review\ndescription: Review code\n{extra}---\n{body}"
    ))
    .unwrap()
}

#[test]
fn local_values_and_deletions_survive_repeated_upstream_updates() {
    let original = document("license: MIT\ncompatibility: old\n", "old\n");
    let current = document(
        "disable-model-invocation: true\ncompatibility: old\nmetadata:\n  nested: [one, two]\n",
        "old\n",
    );
    let upstream = document(
        "license: Apache\ncompatibility: new\ndisable-model-invocation: false\n",
        "new\n",
    );
    let (merged, state) = MetadataState::new(&original)
        .merge(Some(&current), &upstream)
        .unwrap();
    assert_eq!(merged.fields["disable-model-invocation"], Value::Bool(true));
    assert!(!merged.fields.contains_key("license"));
    assert_eq!(merged.fields["compatibility"].as_str(), Some("new"));
    assert_eq!(merged.fields["metadata"], current.fields["metadata"]);
    assert_eq!(merged.body, "new\n");
    let coincident = document("disable-model-invocation: true\nlicense: MIT\n", "next\n");
    let (second, state) = state.merge(Some(&merged), &coincident).unwrap();
    let (third, _) = state.merge(Some(&second), &upstream).unwrap();
    assert_eq!(third.fields["disable-model-invocation"], Value::Bool(true));
    assert!(!third.fields.contains_key("license"));
}

#[test]
fn metadata_proof_protects_body_identity_assets_and_modes() {
    let temp = tempfile::tempdir().unwrap();
    let original = document("", "instructions\n");
    std::fs::write(temp.path().join("SKILL.md"), original.bytes()).unwrap();
    std::fs::write(temp.path().join("asset"), "asset").unwrap();
    let digest = canonical_skill_digest(temp.path()).unwrap();
    let state = MetadataState::new(&original);
    let current = document("disable-model-invocation: true\n", "instructions\n");
    std::fs::write(temp.path().join("SKILL.md"), current.bytes()).unwrap();
    assert!(state.matches(temp.path(), &current, &digest).unwrap());
    for text in [
        String::from_utf8(current.bytes())
            .unwrap()
            .replace("name: review", "name: other"),
        String::from_utf8(current.bytes())
            .unwrap()
            .replace("Review code", "Different"),
        String::from_utf8(current.bytes())
            .unwrap()
            .replace("instructions", "changed"),
    ] {
        let changed = Document::parse(&text).unwrap();
        assert!(!state.matches(temp.path(), &changed, &digest).unwrap());
    }
    std::fs::write(temp.path().join("asset"), "changed").unwrap();
    assert!(!state.matches(temp.path(), &current, &digest).unwrap());
    std::fs::write(temp.path().join("asset"), "asset").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = temp.path().join("SKILL.md");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!state.matches(temp.path(), &current, &digest).unwrap());
    }
}

#[test]
fn preserves_crlf_comments_and_multiline_fields_on_noop() {
    let text = "---\r\nname: review\r\ndescription: |\r\n  Review code\r\n# local comment\r\ndisable-model-invocation: true\r\n---\r\nBody\r\n";
    let current = Document::parse(text).unwrap();
    let (merged, state) = MetadataState::new(&current)
        .merge(Some(&current), &current)
        .unwrap();
    assert_eq!(merged.bytes(), text.as_bytes());
    let roundtrip: MetadataState = toml::from_str(&toml::to_string(&state).unwrap()).unwrap();
    assert_eq!(state, roundtrip);
}

#[test]
fn rejects_ambiguous_and_unbounded_frontmatter() {
    for extra in [
        "name: duplicate\n",
        "description: duplicate\n",
        "key: one\nkey: two\n",
        "metadata: {key: one, key: two}\n",
        "42: value\n",
        "metadata: !custom value\n",
        "<<: {key: value}\n",
        "metadata: [\n",
    ] {
        let result = Document::parse(&format!(
            "---\nname: review\ndescription: Review\n{extra}---\n"
        ));
        assert!(result.is_err(), "accepted {extra:?}");
    }
    let deep = format!("key: {}0{}\n", "[".repeat(40), "]".repeat(40));
    assert!(parse_yaml(&deep).is_err());
    assert!(Document::parse(&"a".repeat(SKILL_MD_MAX_BYTES as usize + 1)).is_err());
}

#[test]
fn yaml_alias_expansion_is_bounded_during_parsing() {
    assert_eq!(
        parse_yaml("one: &value [true, null, 42, 1.5]\ntwo: *value\n").unwrap()["one"],
        parse_yaml("[true, null, 42, 1.5]").unwrap()
    );
    let mut bomb = "v0: &v0 [a, b, c, d]\n".to_owned();
    for level in 1..10 {
        bomb.push_str(&format!(
            "v{level}: &v{level} [{}]\n",
            vec![format!("*v{}", level - 1); 10].join(", ")
        ));
    }
    assert!(
        parse_yaml(&bomb)
            .unwrap_err()
            .to_string()
            .contains("structure limits")
    );
    let large_string = format!(
        "value: &value {}\ncopies: [{}]\n",
        "x".repeat(100_000),
        ["*value"; 11].join(", ")
    );
    assert!(
        parse_yaml(&large_string)
            .unwrap_err()
            .to_string()
            .contains("expanded YAML byte limit")
    );
}

#[test]
fn body_can_be_added_after_a_header_without_a_final_newline() {
    let original = Document::parse("---\nname: review\ndescription: Review code\n---").unwrap();
    let upstream = document("", "new body\n");
    let (merged, _) = MetadataState::new(&original)
        .merge(Some(&original), &upstream)
        .unwrap();
    assert_eq!(merged.body, "new body\n");
}

#[test]
fn skill_metadata_state_matches_golden_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let original = document("license: MIT\n", "Instructions\n");
    std::fs::write(temp.path().join("SKILL.md"), original.bytes()).unwrap();
    let source_digest = canonical_skill_digest(temp.path()).unwrap();
    let current = document(
        "disable-model-invocation: true\nmetadata: {category: local}\n",
        "Instructions\n",
    );
    let (_, mut metadata) = MetadataState::new(&original)
        .merge(Some(&current), &original)
        .unwrap();
    metadata.source_digest = source_digest;
    let state = crate::ownership::State {
        version: 2,
        entries: vec![crate::ownership::StateEntry {
            destination: ".pi/skills/review".into(),
            kind: "skill".into(),
            key: "review".into(),
            mode: "copy".into(),
            last_applied_digest: skill_digest_with_document(temp.path(), &current.bytes()).unwrap(),
            lock_identity: crate::digest::sha256_bytes(b"fixture lock identity"),
            skill_metadata: Some(metadata),
        }],
    };
    let bytes = state.bytes().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/contracts/state-skill-metadata.toml");
    if std::env::var_os("ARU_UPDATE_CONTRACTS").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::write(&fixture, &bytes).unwrap();
    }
    assert_eq!(bytes, std::fs::read(fixture).unwrap());
    let roundtrip: crate::ownership::State =
        toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(roundtrip.bytes().unwrap(), bytes);
}

#[test]
fn missing_projection_retains_recorded_overrides() {
    let original = document("", "old");
    let current = document("disable-model-invocation: true\n", "old");
    let (_, state) = MetadataState::new(&original)
        .merge(Some(&current), &original)
        .unwrap();
    let (restored, _) = state.merge(None, &original).unwrap();
    assert_eq!(
        restored.fields["disable-model-invocation"],
        Value::Bool(true)
    );
}
