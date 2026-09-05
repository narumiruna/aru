use super::*;

#[test]
fn oversized_state_is_rejected_without_replacing_existing_content() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("transaction.toml");
    std::fs::write(&path, "preserve").unwrap();
    assert!(write_atomic(&path, &"x".repeat(LIMIT as usize + 1)).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"preserve");
    assert!(!path.with_extension("toml.tmp").exists());
    let file = OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(LIMIT + 1).unwrap();
    assert!(read(&path).is_err());
    assert_eq!(path.metadata().unwrap().len(), LIMIT + 1);
}

#[test]
fn foreign_ownership_is_rejected_without_root_privileges() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("journal");
    std::fs::write(&path, "pending").unwrap();
    let metadata = path.metadata().unwrap();
    let uid = unsafe { libc::geteuid() };
    assert!(owned_private_file(&metadata, uid));
    assert!(!owned_private_file(&metadata, uid.wrapping_add(1)));
}

#[test]
fn injected_committed_journal_cannot_delete_an_unrelated_path() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    let victim = root.join("victim");
    std::fs::write(&victim, b"preserve").unwrap();
    let destination = root.join("absent");
    let journal = Journal {
        version: 2,
        phase: "committed".into(),
        root: None,
        entries: vec![JournalEntry {
            destination: encode_absolute_path(&destination).unwrap(),
            stage: Some(encode_absolute_path(&victim).unwrap()),
            backup: None,
            old_digest: None,
            new_digest: None,
            applied: true,
        }],
    };
    let payload = root.join("payload");
    std::fs::write(&payload, toml::to_string_pretty(&journal).unwrap()).unwrap();
    let path = root.join("transaction.toml");
    std::os::unix::fs::symlink(&payload, &path).unwrap();
    assert!(recover_standalone_if_needed_at(&path).is_err());
    assert_eq!(std::fs::read(&victim).unwrap(), b"preserve");
    assert!(path.is_symlink());
}

#[test]
fn temporary_journal_links_are_not_truncated_or_followed() {
    for hardlink in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("transaction.toml");
        let temporary = path.with_extension("toml.tmp");
        let victim = root.path().join("victim");
        std::fs::write(&victim, "preserve").unwrap();
        if hardlink {
            std::fs::hard_link(&victim, &temporary).unwrap();
        } else {
            std::os::unix::fs::symlink(&victim, &temporary).unwrap();
        }
        assert!(write_atomic(&path, "new").is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"preserve");
        assert!(!path.exists());
    }
}
