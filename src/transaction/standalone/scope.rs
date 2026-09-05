//! Pin the selected operation scope outside the account home before any journal writes.
use super::*;
use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scope {
    version: u32,
    control: String,
    device: u64,
    inode: u64,
}

#[cfg(not(test))]
pub(super) fn select(
    home: Option<&Path>,
    uid: libc::uid_t,
    project: Option<&Path>,
    preview: bool,
) -> Result<(Option<File>, PathBuf)> {
    let fallback = unix_control_directory(None, uid)?;
    let anchor = fallback.with_file_name(format!("aru-standalone-scope-{uid}"));
    select_at(home, uid, project, preview, &anchor, &fallback)
}

fn select_at(
    home: Option<&Path>,
    uid: libc::uid_t,
    project: Option<&Path>,
    preview: bool,
    anchor: &Path,
    fallback: &Path,
) -> Result<(Option<File>, PathBuf)> {
    reject_project_control(anchor, project)?;
    prepare_control_directory(anchor, !preview)?;
    let guard = acquire_lock_file(&anchor.join("scope.lock"), true)?;
    let marker = anchor.join("scope.toml");
    if let Some(text) = super::super::state_file::read(&marker)? {
        let scope: Scope = toml::from_str(&text).map_err(|source| AruError::Toml {
            path: marker.clone(),
            source,
        })?;
        if scope.version != 1 {
            return Err(AruError::msg("unsupported standalone scope marker version"));
        }
        let control = super::super::decode_absolute_path(&scope.control)?;
        reject_project_control(&control, project)?;
        validate_control_ancestors(&control)?;
        let metadata = control.symlink_metadata().at(&control)?;
        if !metadata.is_dir()
            || metadata.uid() != uid
            || metadata.dev() != scope.device
            || metadata.ino() != scope.inode
        {
            return Err(AruError::msg(
                "pinned transaction scope is unavailable or was replaced; restore its original filesystem before retrying",
            ));
        }
        return Ok((Some(guard), control));
    }

    // Without an anchor, an unavailable existing account home is ambiguous:
    // it may hide an older journal. Never initialize a second operation lock.
    if !established_fallback_scope(fallback, uid)
        && home.is_some_and(|home| home.is_absolute() && !home.is_dir())
    {
        return Err(AruError::msg(
            "cannot establish transaction scope while the account home is unavailable; restore it before retrying",
        ));
    }
    let mut control = select_unix_control_directory(home, uid, fallback)?;
    if control_overlaps_project(&control, project)? {
        if control.join("operation.lock").symlink_metadata().is_ok()
            || control.join("transaction.toml").symlink_metadata().is_ok()
        {
            return Err(AruError::msg(
                "existing transaction state overlaps this project; refusing to switch its recovery scope",
            ));
        }
        control = fallback.to_path_buf();
    }
    reject_project_control(&control, project)?;
    prepare_control_directory(&control, !preview)?;
    let metadata = control.symlink_metadata().at(&control)?;
    let scope = Scope {
        version: 1,
        control: super::super::encode_absolute_path(&control)?,
        device: metadata.dev(),
        inode: metadata.ino(),
    };
    let text = toml::to_string_pretty(&scope).map_err(|error| {
        AruError::msg(format!("could not serialize transaction scope: {error}"))
    })?;
    super::super::state_file::write_atomic(&marker, &text)?;
    Ok((Some(guard), control))
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
