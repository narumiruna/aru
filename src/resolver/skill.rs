use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::cache::Cache;
use crate::error::{AruError, Result};
use crate::lockfile::{LockedSkill, Lockfile, SkillPackage};
use crate::manifest::{Manifest, SkillRequirement, Target};
use crate::skill::{DiscoveredSkill, discover_and_select, discover_candidates};
use crate::source::git::{self, GitSource};
use crate::target;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillResolutionHint {
    pub requirement: String,
    pub version: String,
    pub revision: String,
}

#[derive(Debug, Clone)]
pub struct SkillSourceInspection {
    pub source: String,
    pub requirement: String,
    pub version: String,
    pub revision: String,
    pub candidates: Vec<DiscoveredSkill>,
}

impl SkillSourceInspection {
    pub fn hint(&self) -> SkillResolutionHint {
        SkillResolutionHint {
            requirement: self.requirement.clone(),
            version: self.version.clone(),
            revision: self.revision.clone(),
        }
    }
}

pub(super) struct SkillSourceCatalog {
    sources: BTreeMap<String, GitSource>,
}

impl SkillSourceCatalog {
    pub(super) fn canonicalize(
        project: &Path,
        requirements: &BTreeMap<String, SkillRequirement>,
    ) -> Result<Self> {
        let mut sources = BTreeMap::new();
        let mut identities = BTreeMap::<String, String>::new();
        for source in requirements.keys() {
            let canonical = git::canonicalize(project, source)?;
            if let Some(previous) = identities.insert(canonical.identity.clone(), source.clone()) {
                return Err(AruError::msg(format!(
                    "skill sources {previous:?} and {source:?} identify the same repository"
                )));
            }
            sources.insert(source.clone(), canonical);
        }
        Ok(Self { sources })
    }

    pub(super) fn as_map(&self) -> &BTreeMap<String, GitSource> {
        &self.sources
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &GitSource> {
        self.sources.values()
    }

    fn get(&self, declared: &str) -> &GitSource {
        self.sources
            .get(declared)
            .expect("direct Skill source catalog covers manifest requirements")
    }
}

pub(super) struct DirectSkillResolution {
    pub(super) packages: Vec<SkillPackage>,
    pub(super) sources: BTreeMap<String, PathBuf>,
}

pub(super) struct DirectSkillOptions<'a> {
    pub(super) previous: Option<&'a Lockfile>,
    pub(super) offline: bool,
    pub(super) updates: &'a BTreeSet<String>,
    pub(super) hints: &'a BTreeMap<String, SkillResolutionHint>,
}

pub(super) fn validate_project_targets(manifest: &Manifest) -> Result<()> {
    if !manifest.skills.is_empty() && target::skill_targets(&manifest.project.targets).is_empty() {
        return Err(AruError::msg(
            "no configured target supports Agent Skills projections",
        ));
    }
    Ok(())
}

pub(super) fn resolve(
    manifest: &Manifest,
    catalog: &SkillSourceCatalog,
    cache: &Cache,
    options: DirectSkillOptions<'_>,
) -> Result<DirectSkillResolution> {
    let mut packages = Vec::new();
    let mut sources = BTreeMap::new();
    for (manifest_source, requirement) in &manifest.skills {
        let source = catalog.get(manifest_source);
        let descriptor = skill_requirement_descriptor(requirement);
        let targets = effective_targets(&manifest.project.targets, requirement.targets.as_deref());
        let old = options.previous.and_then(|lock| {
            lock.skill_packages
                .iter()
                .find(|package| package.source == source.identity)
        });
        let update = options.updates.contains(&source.identity);
        let (version, revision) = if let Some(hint) = options.hints.get(&source.identity) {
            validate_skill_hint(hint, &source.identity, requirement)?;
            (hint.version.clone(), hint.revision.clone())
        } else {
            resolve_skill_reference(source, requirement, old, update, options.offline)?
        };
        let checkout = cache.checkout_with_policy(source, &revision, options.offline)?;
        let mut selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
        if let Some(old) = old.filter(|_| !update)
            && selected_digest_mismatch(&selected, old)
        {
            cache.invalidate(source, &revision)?;
            let checkout = cache.checkout_with_policy(source, &revision, options.offline)?;
            selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
            if selected_digest_mismatch(&selected, old) {
                return Err(AruError::msg(format!(
                    "content for locked Git revision {} does not match aru.lock",
                    revision
                )));
            }
        }
        for skill in &selected {
            if sources
                .insert(skill.name.clone(), skill.absolute_path.clone())
                .is_some()
            {
                return Err(AruError::msg(format!(
                    "resolved skill name {:?} is provided by more than one package",
                    skill.name
                )));
            }
        }
        packages.push(SkillPackage {
            source: source.identity.clone(),
            requirement: descriptor,
            version,
            revision,
            repository_name: source.repository_name.clone(),
            targets,
            skills: selected.iter().map(locked_skill).collect(),
        });
    }
    Ok(DirectSkillResolution { packages, sources })
}

pub(super) fn validate_locked_targets(
    lock: &Lockfile,
    manifest: &Manifest,
    catalog: &SkillSourceCatalog,
) -> Result<()> {
    for (manifest_source, requirement) in &manifest.skills {
        let source = catalog.get(manifest_source);
        let package = lock
            .skill_packages
            .iter()
            .find(|package| package.source == source.identity)
            .ok_or_else(|| AruError::msg("aru.lock is missing a skill package"))?;
        let expected = effective_targets(&manifest.project.targets, requirement.targets.as_deref());
        if package.targets != expected {
            return Err(AruError::msg(format!(
                "aru.lock lacks complete per-target projection selection for skill source {:?}",
                manifest_source
            )));
        }
    }
    Ok(())
}

pub(super) fn locked_sources(
    cache: &Cache,
    manifest: &Manifest,
    catalog: &SkillSourceCatalog,
    lock: &Lockfile,
    materialize: bool,
    offline: bool,
) -> Result<BTreeMap<String, PathBuf>> {
    if !materialize {
        let package_sources = lock
            .aru_packages
            .iter()
            .map(|package| package.source.as_str())
            .collect::<BTreeSet<_>>();
        return Ok(lock
            .skill_packages
            .iter()
            .filter(|package| !package_sources.contains(package.source.as_str()))
            .flat_map(|package| package.skills.iter())
            .map(|skill| (skill.name.clone(), PathBuf::new()))
            .collect());
    }

    let mut output = BTreeMap::new();
    for (manifest_source, requirement) in &manifest.skills {
        let source = catalog.get(manifest_source);
        let package = lock
            .skill_packages
            .iter()
            .find(|package| package.source == source.identity)
            .ok_or_else(|| AruError::msg("aru.lock is missing a skill package"))?;
        let mut checkout = cache.checkout_with_policy(source, &package.revision, offline)?;
        let mut selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
        if selected_matches_lock(&selected, package).is_err() {
            cache.invalidate(source, &package.revision)?;
            checkout = cache.checkout_with_policy(source, &package.revision, offline)?;
            selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
            selected_matches_lock(&selected, package)?;
        }
        for skill in selected {
            if output
                .insert(skill.name.clone(), skill.absolute_path)
                .is_some()
            {
                return Err(AruError::msg("aru.lock resolves duplicate skill names"));
            }
        }
    }
    Ok(output)
}

pub fn inspect_skill_source(
    project: &Path,
    manifest_source: &str,
    requirement: &SkillRequirement,
    previous: Option<&Lockfile>,
    dry_run: bool,
    offline: bool,
) -> Result<SkillSourceInspection> {
    let source = git::canonicalize(project, manifest_source)?;
    let descriptor = skill_requirement_descriptor(requirement);
    let old = previous.and_then(|lock| {
        lock.skill_packages
            .iter()
            .find(|package| package.source == source.identity)
    });
    let (version, revision) = resolve_skill_reference(&source, requirement, old, false, offline)?;
    let cache = if dry_run {
        Cache::ephemeral_for_project(project)?
    } else {
        Cache::project(project)
    };
    let mut checkout = cache.checkout_with_policy(&source, &revision, offline)?;
    let mut candidates =
        discover_candidates(&checkout, &source.repository_name, &requirement.paths)?;
    if let Some(old) = old
        && selected_digest_mismatch(&candidates, old)
    {
        cache.invalidate(&source, &revision)?;
        checkout = cache.checkout_with_policy(&source, &revision, offline)?;
        candidates = discover_candidates(&checkout, &source.repository_name, &requirement.paths)?;
        if selected_digest_mismatch(&candidates, old) {
            return Err(AruError::msg(format!(
                "content for locked Git revision {} does not match aru.lock",
                revision
            )));
        }
    }
    Ok(SkillSourceInspection {
        source: source.identity,
        requirement: descriptor,
        version,
        revision,
        candidates,
    })
}

pub fn canonical_update_skill_targets(
    project: &Path,
    manifest: &Manifest,
    requested: &[String],
) -> Result<BTreeSet<String>> {
    if requested.is_empty() {
        return manifest
            .skills
            .keys()
            .map(|source| git::canonicalize(project, source).map(|source| source.identity))
            .collect();
    }
    let catalog = SkillSourceCatalog::canonicalize(project, &manifest.skills)?;
    let mut output = BTreeSet::new();
    for request in requested {
        let canonical = git::canonicalize(project, request)?;
        if !catalog
            .values()
            .any(|source| source.identity == canonical.identity)
        {
            return Err(AruError::msg(format!(
                "skill source {request:?} is not declared in aru.toml"
            )));
        }
        output.insert(canonical.identity);
    }
    Ok(output)
}

pub(crate) fn declared_skill_source_key(
    project: &Path,
    manifest: &Manifest,
    requested: &str,
) -> Result<Option<String>> {
    if manifest.skills.contains_key(requested) {
        return Ok(Some(requested.into()));
    }
    let canonical = git::canonicalize(project, requested)?;
    for key in manifest.skills.keys() {
        if git::canonicalize(project, key)?.identity == canonical.identity {
            return Ok(Some(key.clone()));
        }
    }
    Ok(None)
}

fn resolve_skill_reference(
    source: &GitSource,
    requirement: &SkillRequirement,
    old: Option<&SkillPackage>,
    update: bool,
    offline: bool,
) -> Result<(String, String)> {
    let old = old.map(|package| git::LockedReference {
        requirement: &package.requirement,
        version: &package.version,
        revision: &package.revision,
    });
    let resolved = git::select_reference(
        source,
        skill_reference(requirement),
        old,
        git::ReferencePolicy {
            update,
            offline,
            ..git::ReferencePolicy::default()
        },
        &format!("Git source {}", source.identity),
    )?;
    Ok((resolved.version, resolved.revision))
}

fn validate_skill_hint(
    hint: &SkillResolutionHint,
    source_identity: &str,
    requirement: &SkillRequirement,
) -> Result<()> {
    if hint.requirement != skill_requirement_descriptor(requirement) {
        return Err(AruError::msg(format!(
            "interactive skill preview for {source_identity:?} no longer matches the requirement"
        )));
    }
    let revision_valid =
        hint.revision.len() == 40 && hint.revision.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !revision_valid
        || requirement
            .rev
            .as_ref()
            .is_some_and(|revision| !hint.revision.starts_with(&revision.to_ascii_lowercase()))
        || requirement
            .branch
            .as_ref()
            .is_some_and(|branch| hint.version != *branch)
        || requirement.version.as_deref().is_some_and(|_| {
            !git::locked_version_matches(requirement.version.as_deref(), &hint.version)
        })
    {
        return Err(AruError::msg(format!(
            "interactive skill preview for {source_identity:?} has an invalid resolved revision"
        )));
    }
    Ok(())
}

fn effective_targets(project_targets: &[Target], selected: Option<&[Target]>) -> Vec<Target> {
    let mut targets = selected.unwrap_or(project_targets).to_vec();
    targets.retain(|target| target::capabilities(*target).skills);
    targets.sort();
    targets.dedup();
    targets
}

fn skill_reference(requirement: &SkillRequirement) -> git::ReferenceSpec<'_> {
    git::ReferenceSpec::new(
        requirement.version.as_deref(),
        requirement.branch.as_deref(),
        requirement.rev.as_deref(),
    )
}

fn skill_requirement_descriptor(requirement: &SkillRequirement) -> String {
    skill_reference(requirement).descriptor()
}

fn locked_skill(skill: &DiscoveredSkill) -> LockedSkill {
    LockedSkill {
        name: skill.name.clone(),
        path: skill.relative_path.clone(),
        sha256: skill.sha256.clone(),
    }
}

fn selected_digest_mismatch(selected: &[DiscoveredSkill], old: &SkillPackage) -> bool {
    selected.iter().any(|skill| {
        old.skills
            .iter()
            .find(|locked| locked.name == skill.name && locked.path == skill.relative_path)
            .is_some_and(|locked| locked.sha256 != skill.sha256)
    })
}

fn selected_matches_lock(selected: &[DiscoveredSkill], package: &SkillPackage) -> Result<()> {
    let found: Vec<_> = selected.iter().map(locked_skill).collect();
    if found == package.skills {
        Ok(())
    } else {
        Err(AruError::msg(format!(
            "materialized skill content for {} does not match aru.lock",
            package.source
        )))
    }
}

#[cfg(test)]
mod tests;
