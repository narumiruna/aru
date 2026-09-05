use super::*;
use unicode_normalization::UnicodeNormalization;

pub(super) fn validate_operations(
    mode: PathMode<'_>,
    operations: &[Operation],
    journal_version: u32,
) -> Result<()> {
    let mut destinations = Vec::with_capacity(operations.len());
    for operation in operations {
        validate_destination(mode, &operation.destination)?;
        validate_ancestors(mode, &operation.destination)?;
        let resolved = resolve_path(mode, &operation.destination);
        encode_journal_path(journal_version, mode, &resolved)?;
        let normalized = normalize_destination(&resolved)?;
        destinations.push((portable_identity(&normalized), normalized));
    }
    destinations.sort_unstable();
    for pair in destinations.windows(2) {
        let [(left_key, left), (right_key, right)] = pair else {
            unreachable!("destination windows always contain two paths")
        };
        if left_key == right_key {
            return Err(AruError::msg(format!(
                "transaction contains duplicate destination or case-ambiguous paths: {} and {}",
                left.display(),
                right.display()
            )));
        }
        if right_key.starts_with(left_key) {
            return Err(AruError::msg(format!(
                "transaction destinations must not be nested (including case-ambiguous paths): {} and {}",
                left.display(),
                right.display()
            )));
        }
    }
    Ok(())
}

// No portable, read-only query describes case/normalization semantics for every
// filesystem (including missing directories and network mounts). Fail closed:
// reject case/normalization-ambiguous plans even on case-sensitive filesystems.
// Keep the actual destination and journal paths lossless and unchanged.
fn portable_identity(path: &Path) -> Vec<Vec<u8>> {
    path.components()
        .map(|component| {
            let mut key = Vec::new();
            for chunk in component.as_os_str().as_encoded_bytes().utf8_chunks() {
                let folded: String = chunk
                    .valid()
                    .nfd()
                    .flat_map(char::to_uppercase)
                    .flat_map(char::to_lowercase)
                    .nfd()
                    .collect();
                key.extend_from_slice(folded.as_bytes());
                key.extend_from_slice(chunk.invalid());
            }
            key
        })
        .collect()
}

pub(super) fn normalize_destination(path: &Path) -> Result<PathBuf> {
    let mut existing = path.parent().unwrap_or(path);
    let mut suffix = path
        .file_name()
        .into_iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    loop {
        match std::fs::symlink_metadata(existing) {
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let component = existing.file_name().ok_or_else(|| {
                    AruError::msg(format!(
                        "could not find an existing destination ancestor for {}",
                        path.display()
                    ))
                })?;
                suffix.push(PathBuf::from(component));
                existing = existing.parent().ok_or_else(|| {
                    AruError::msg(format!(
                        "could not find an existing destination ancestor for {}",
                        path.display()
                    ))
                })?;
            }
            Err(error) => {
                return Err(AruError::msg(format!(
                    "could not inspect destination ancestor {}: {error}",
                    existing.display()
                )));
            }
        }
    }
    let mut normalized = existing.canonicalize().at(existing)?;
    for component in suffix.into_iter().rev() {
        normalized.push(component);
    }
    Ok(normalized)
}
