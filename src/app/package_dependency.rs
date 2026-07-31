use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cli::{PackageAddArgs, PackageRemoveArgs, PackageUpdateArgs};
use crate::error::{AruError, Result};
use crate::lockfile::Lockfile;
use crate::manifest::{Manifest, ManifestDocument, validate_name};
use crate::sync::{CollisionPolicy, UpdateSelection};

use super::{ExecutionPolicy, ProjectionPolicy, begin, execute};

pub(super) fn add(project: &Path, args: PackageAddArgs, policy: ExecutionPolicy) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let key = find_package_key(project, &manifest, &args.source)?.unwrap_or(args.source.clone());
    let mut requirement = manifest.packages.get(&key).cloned().unwrap_or_default();
    if let Some(version) = args.version {
        requirement.version = Some(version);
        requirement.branch = None;
        requirement.rev = None;
    }
    if let Some(branch) = args.branch {
        requirement.branch = Some(branch);
        requirement.version = None;
        requirement.rev = None;
    }
    if let Some(revision) = args.rev {
        requirement.rev = Some(revision);
        requirement.version = None;
        requirement.branch = None;
    }
    if !args.targets.is_empty() {
        requirement.targets = Some(args.targets);
    }
    requirement.normalize();
    requirement.validate(&key, &manifest.project.targets)?;
    document.set_package(&key, &requirement);

    if !args.trust_mcp.is_empty() {
        let trust_key = find_trust_key(project, &manifest, &args.source)?.unwrap_or(key.clone());
        let mut trust = manifest
            .package_trust
            .get(&trust_key)
            .cloned()
            .unwrap_or_default();
        for name in args.trust_mcp {
            validate_name(&name, "trusted package MCP name")?;
            trust.mcp.push(name);
        }
        trust.normalize();
        trust.validate(&trust_key)?;
        document.set_package_trust(&trust_key, &trust);
    }

    let manifest = document.manifest()?;
    let updates = if args.upgrade {
        BTreeSet::from([crate::source::git::canonicalize(project, &key)?.identity])
    } else {
        BTreeSet::new()
    };
    let request = policy
        .request(
            args.dry_run,
            package_projection(args.no_sync, args.merge, args.force)?,
        )
        .with_manifest_bytes(document.bytes())
        .with_updates(UpdateSelection::default().packages(updates, BTreeMap::new()));
    execute(project, &manifest, request, policy.output)
}

pub(super) fn remove(
    project: &Path,
    args: PackageRemoveArgs,
    policy: ExecutionPolicy,
) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let key = find_package_key(project, &manifest, &args.source)?.ok_or_else(|| {
        AruError::msg(format!(
            "aru package source {:?} is not declared",
            args.source
        ))
    })?;
    document.remove_package(&key);
    if let Some(trust_key) = find_trust_key(project, &manifest, &args.source)? {
        document.remove_package_trust(&trust_key);
    }
    let manifest = document.manifest()?;
    let request = policy
        .request(
            args.dry_run,
            package_projection(args.no_sync, false, false)?,
        )
        .with_manifest_bytes(document.bytes());
    execute(project, &manifest, request, policy.output)
}

pub(super) fn update(
    project: &Path,
    args: PackageUpdateArgs,
    policy: ExecutionPolicy,
) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    if manifest.packages.is_empty() {
        return Err(AruError::msg("aru.toml declares no native aru packages"));
    }
    let previous = Lockfile::load_optional(project)?;
    let available = previous
        .as_ref()
        .map(|lock| {
            lock.aru_packages
                .iter()
                .map(|package| package.source.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_else(|| {
            manifest
                .packages
                .keys()
                .filter_map(|source| {
                    crate::source::git::canonicalize(project, source)
                        .ok()
                        .map(|source| source.identity)
                })
                .collect()
        });
    let updates = if args.packages.is_empty() {
        available.clone()
    } else {
        let mut selected = BTreeSet::new();
        for requested in &args.packages {
            let canonical = crate::source::git::canonicalize(project, requested)?;
            if !available.contains(&canonical.identity) {
                return Err(AruError::msg(format!(
                    "aru package {requested:?} is not present in the locked package graph"
                )));
            }
            selected.insert(canonical.identity);
        }
        selected
    };
    let precise = if let Some(version) = args.precise {
        if updates.len() != 1 {
            return Err(AruError::msg(
                "--precise requires exactly one selected aru package",
            ));
        }
        BTreeMap::from([(updates.iter().next().unwrap().clone(), version)])
    } else {
        BTreeMap::new()
    };
    let request = policy
        .request(
            args.dry_run,
            package_projection(args.no_sync, args.merge, args.force)?,
        )
        .with_updates(UpdateSelection::default().packages(updates, precise));
    execute(project, &manifest, request, policy.output)
}

fn package_projection(no_sync: bool, merge: bool, force: bool) -> Result<ProjectionPolicy> {
    if no_sync {
        Ok(ProjectionPolicy::LockOnly)
    } else {
        Ok(ProjectionPolicy::Project(CollisionPolicy::from_flags(
            merge, force,
        )?))
    }
}

fn find_package_key(
    project: &Path,
    manifest: &Manifest,
    requested: &str,
) -> Result<Option<String>> {
    if manifest.packages.contains_key(requested) {
        return Ok(Some(requested.into()));
    }
    let canonical = crate::source::git::canonicalize(project, requested)?;
    for key in manifest.packages.keys() {
        if crate::source::git::canonicalize(project, key)?.identity == canonical.identity {
            return Ok(Some(key.clone()));
        }
    }
    Ok(None)
}

fn find_trust_key(project: &Path, manifest: &Manifest, requested: &str) -> Result<Option<String>> {
    let canonical = crate::source::git::canonicalize(project, requested)?;
    for key in manifest.package_trust.keys() {
        if crate::source::git::canonicalize(project, key)?.identity == canonical.identity {
            return Ok(Some(key.clone()));
        }
    }
    Ok(None)
}
