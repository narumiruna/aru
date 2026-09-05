use std::path::Path;

use crate::cache::Cache;
use crate::error::{AruError, Result};
use crate::lockfile::Lockfile;
use crate::ownership::StateEntry;
use crate::skill::metadata::{Document, MetadataState};
use crate::skill::{canonical_skill_digest, skill_digest_with_document};

pub(super) struct Projection {
    pub document: Vec<u8>,
    pub digest: String,
    pub metadata: Option<MetadataState>,
    pub owned: Option<StateEntry>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn prepare(
    project: &Path,
    previous: Option<&Lockfile>,
    source: &Path,
    source_digest: &str,
    destination: &Path,
    current_digest: Option<&str>,
    owned: Option<&StateEntry>,
) -> Result<Projection> {
    if source.as_os_str().is_empty() {
        return check(project, destination, source_digest, current_digest, owned);
    }
    let source_document = Document::read(&source.join("SKILL.md"))?;
    let mut reconciled_owned = owned.cloned();
    let current_root =
        if owned.is_some() && current_digest.is_some_and(|digest| digest.starts_with("sha256:")) {
            Some(project.join(destination).canonicalize().map_err(|error| {
                AruError::msg(format!("could not resolve skill destination: {error}"))
            })?)
        } else {
            None
        };
    let current = current_root
        .as_ref()
        .map(|root| Document::read(&root.join("SKILL.md")))
        .transpose()?;
    let metadata = match owned {
        Some(owned) => match &owned.skill_metadata {
            Some(metadata) => metadata.clone(),
            None if current.is_some()
                && current_digest == Some(owned.last_applied_digest.as_str()) =>
            {
                MetadataState::new(current.as_ref().expect("observed owned skill"))
            }
            None if current.is_some() => legacy_metadata(project, previous, source, owned)?,
            None => MetadataState::new(&source_document),
        },
        None => MetadataState::new(&source_document),
    };
    if let (Some(root), Some(current), Some(owned)) =
        (&current_root, &current, &mut reconciled_owned)
    {
        if !metadata.matches(root, current, &owned.last_applied_digest)? {
            return Err(AruError::msg(format!(
                "drift: aru-owned skill {:?} has modified name, description, body, or files",
                owned.key
            )));
        }
        // Only the metadata-specific proof above can authorize this observation.
        owned.last_applied_digest = current_digest.expect("observed owned skill").into();
    }
    let (document, mut metadata) = metadata.merge(current.as_ref(), &source_document)?;
    metadata.source_digest = source_digest.into();
    let document = document.bytes();
    let digest = skill_digest_with_document(source, &document)?;
    Ok(Projection {
        document,
        digest,
        metadata: Some(metadata),
        owned: reconciled_owned,
    })
}

// Exact-state checks must remain offline and work without a source cache.
fn check(
    project: &Path,
    destination: &Path,
    source_digest: &str,
    current_digest: Option<&str>,
    owned: Option<&StateEntry>,
) -> Result<Projection> {
    let mut projection = Projection {
        document: Vec::new(),
        digest: source_digest.into(),
        metadata: owned.and_then(|entry| entry.skill_metadata.clone()),
        owned: owned.cloned(),
    };
    if let (Some(metadata), Some(owned)) = (&projection.metadata, &mut projection.owned) {
        if metadata.source_digest != source_digest {
            return Err(AruError::msg(
                "skill projection source changed; run `aru sync`",
            ));
        }
        projection.digest = owned.last_applied_digest.clone();
        if current_digest.is_some_and(|digest| digest.starts_with("sha256:")) {
            let root = project.join(destination).canonicalize().map_err(|error| {
                AruError::msg(format!("could not resolve skill destination: {error}"))
            })?;
            let current = Document::read(&root.join("SKILL.md"))?;
            if !metadata.matches(&root, &current, &owned.last_applied_digest)? {
                return Err(AruError::msg(format!(
                    "drift: aru-owned skill {:?} has modified name, description, body, or files",
                    owned.key
                )));
            }
            let applied = Document::parse(&format!("{}{}", metadata.frontmatter, current.body))?;
            let (_, next) = metadata.merge(Some(&current), &applied)?;
            projection.metadata = Some(next);
            projection.digest = current_digest.expect("observed owned skill").into();
            owned.last_applied_digest = projection.digest.clone();
        }
    }
    Ok(projection)
}

fn legacy_metadata(
    project: &Path,
    previous: Option<&Lockfile>,
    source: &Path,
    owned: &StateEntry,
) -> Result<MetadataState> {
    // Existing v1 entries have no header snapshot. Bootstrap only from a complete
    // tree matching their last-applied digest, never from the edited projection.
    if canonical_skill_digest(source)? == owned.last_applied_digest {
        return Ok(MetadataState::new(&Document::read(
            &source.join("SKILL.md"),
        )?));
    }
    let cache = Cache::project(project);
    for package in previous.into_iter().flat_map(|lock| &lock.skill_packages) {
        for skill in &package.skills {
            if skill.name != owned.key || skill.sha256 != owned.last_applied_digest {
                continue;
            }
            let plugin = skill.origin.as_ref().and_then(|origin| {
                previous?
                    .plugin_packages
                    .iter()
                    .find(|plugin| plugin.name == origin.name && plugin.source == origin.source)
            });
            let identity = plugin.map_or(package.source.as_str(), |plugin| plugin.source.as_str());
            let Some(root) = cache.cached_content(identity, &package.revision) else {
                continue;
            };
            let relative = Path::new(
                plugin
                    .and_then(|plugin| plugin.subdir.as_deref())
                    .unwrap_or("."),
            )
            .join(&skill.path);
            if relative.components().any(|part| {
                !matches!(
                    part,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            }) {
                return Err(AruError::msg("invalid cached skill path"));
            }
            let source = root.join(relative);
            if canonical_skill_digest(&source)? == owned.last_applied_digest {
                return Ok(MetadataState::new(&Document::read(
                    &source.join("SKILL.md"),
                )?));
            }
        }
    }
    Err(AruError::msg(format!(
        "drift: cannot verify metadata-only edits to {:?} without its last-applied source; restore the prior cached source or original skill before syncing",
        owned.key
    )))
}

pub(super) fn share(projection: &mut Projection, shared: &StateEntry, source: &Path) -> Result<()> {
    if let Some(local) = projection
        .metadata
        .as_ref()
        .filter(|metadata| metadata.has_overrides())
        && !shared.skill_metadata.as_ref().is_some_and(|metadata| {
            local.values == metadata.values && local.removed == metadata.removed
        })
    {
        return Err(AruError::msg(
            "cannot share skill projections with different local metadata overrides; preserve and reconcile the copies before changing targets",
        ));
    }
    if !source.as_os_str().is_empty() {
        let metadata = shared
            .skill_metadata
            .as_ref()
            .ok_or_else(|| AruError::msg("missing shared skill metadata state"))?;
        let source = Document::read(&source.join("SKILL.md"))?;
        projection.document =
            Document::parse(&format!("{}{}", metadata.frontmatter, source.body))?.bytes();
    }
    projection.digest = shared.last_applied_digest.clone();
    projection.metadata = shared.skill_metadata.clone();
    Ok(())
}
