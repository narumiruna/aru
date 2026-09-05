use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::error::{AruError, IoContext, Result};
use crate::skill::{canonical_skill_digest, metadata::Document};

/// Run only after the copied tree has matched the complete locked source digest.
pub(super) fn stage_document(stage: &Path, bytes: &[u8], expected_digest: &str) -> Result<()> {
    let path = stage.join("SKILL.md");
    let source = Document::read(&path)?;
    let projected = Document::parse(
        std::str::from_utf8(bytes).map_err(|_| AruError::msg("skill document is not UTF-8"))?,
    )?;
    if source.body != projected.body
        || ["name", "description"]
            .iter()
            .any(|key| source.fields[*key] != projected.fields[*key])
    {
        return Err(AruError::msg(
            "skill metadata projection changes protected source content",
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(&path)
        .at(&path)?;
    file.write_all(bytes).at(&path)?;
    file.sync_all().at(&path)?;
    if canonical_skill_digest(stage)? != expected_digest {
        return Err(AruError::msg(
            "post-merge skill digest does not match the prepared projection",
        ));
    }
    Ok(())
}
