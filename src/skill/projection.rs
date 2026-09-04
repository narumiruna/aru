use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::error::{AruError, Result};

use super::{
    DISCOVERY_MAX_DEPTH, DISCOVERY_MAX_DIRECTORIES, DISCOVERY_MAX_ENTRIES, SKILL_FILE_MAX_BYTES,
    SKILL_TOTAL_MAX_BYTES, SkillTreeStructuralBudget,
    canonical_skill_digest_with_structural_budget,
};

const SOURCE_LOCK_MAX_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn locked_projection_roots(root: &Path) -> Result<BTreeSet<PathBuf>> {
    locked_projection_roots_with_limits(
        root,
        SOURCE_LOCK_MAX_BYTES,
        DISCOVERY_MAX_DEPTH,
        DISCOVERY_MAX_DIRECTORIES,
        DISCOVERY_MAX_ENTRIES,
    )
}

#[cfg(test)]
pub(super) fn locked_projection_roots_with_limit(
    root: &Path,
    max_lock_bytes: u64,
) -> Result<BTreeSet<PathBuf>> {
    locked_projection_roots_with_limits(
        root,
        max_lock_bytes,
        DISCOVERY_MAX_DEPTH,
        DISCOVERY_MAX_DIRECTORIES,
        DISCOVERY_MAX_ENTRIES,
    )
}

pub(super) fn locked_projection_roots_with_limits(
    root: &Path,
    max_lock_bytes: u64,
    max_depth: usize,
    max_directories: usize,
    max_entries: usize,
) -> Result<BTreeSet<PathBuf>> {
    let path = root.join(crate::lockfile::LOCK_FILE);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(error) => {
            return Err(AruError::Io {
                path,
                source: error,
            });
        }
    };
    if !metadata.file_type().is_file() {
        return Ok(BTreeSet::new());
    }
    if metadata.len() > max_lock_bytes {
        return Err(AruError::msg(format!(
            "source aru.lock is {} bytes; skill discovery limit is {max_lock_bytes} bytes",
            metadata.len()
        )));
    }
    let lock = match crate::lockfile::Lockfile::load_optional(root) {
        Ok(Some(lock)) => lock,
        Ok(None) | Err(_) => return Ok(BTreeSet::new()),
    };
    let mut checked = BTreeSet::new();
    let mut ignored = BTreeSet::new();
    let mut structural_budget =
        SkillTreeStructuralBudget::new(max_depth, max_directories, max_entries);
    for package in lock.skill_packages {
        for skill in package.skills {
            if crate::manifest::validate_name(&skill.name, "skill name").is_err() {
                continue;
            }
            for target in &package.targets {
                let projection = crate::target::spec(*target).project_skills;
                if !projection.starts_with('.') {
                    continue;
                }
                let candidate = root.join(projection).join(&skill.name);
                if checked.insert(candidate.clone())
                    && projection_root_is_safe(root, &candidate)
                    && canonical_projection_digest(&candidate, &mut structural_budget)
                        .is_ok_and(|digest| digest == skill.sha256)
                {
                    ignored.insert(candidate);
                }
            }
        }
    }
    Ok(ignored)
}

fn projection_root_is_safe(root: &Path, candidate: &Path) -> bool {
    let Ok(relative) = candidate.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return false;
        };
        current.push(component);
        if !std::fs::symlink_metadata(&current).is_ok_and(|metadata| metadata.file_type().is_dir())
        {
            return false;
        }
    }
    root.canonicalize()
        .and_then(|root| {
            candidate
                .canonicalize()
                .map(|candidate| candidate.starts_with(root))
        })
        .unwrap_or(false)
}

fn canonical_projection_digest(
    root: &Path,
    structural_budget: &mut SkillTreeStructuralBudget,
) -> Result<String> {
    canonical_skill_digest_with_structural_budget(
        root,
        SKILL_FILE_MAX_BYTES,
        SKILL_TOTAL_MAX_BYTES,
        Some(structural_budget),
    )
}
