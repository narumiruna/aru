use std::collections::BTreeSet;
use std::path::Path;

use crate::cli::{InstructionAddArgs, InstructionRemoveArgs};
use crate::error::{AruError, Result};
use crate::manifest::{InstructionSource, InstructionSourceScope, ManifestDocument};
use crate::sync::CollisionPolicy;

use super::ProjectionPolicy;

pub(super) fn add(
    project: &Path,
    args: InstructionAddArgs,
    policy: super::ExecutionPolicy,
) -> Result<()> {
    let _guard = super::begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let mut files = args.files.clone();
    files.sort();
    files.dedup();
    if let Some(pattern) = files
        .iter()
        .find(|file| file.contains(['*', '?', '[', '{']))
    {
        return Err(AruError::msg(format!(
            "instruction add requires exact AGENTS.md file paths, not glob {pattern:?}; configure globs in aru.toml"
        )));
    }

    let original_sources = manifest.instructions.sources;
    let mut sources = original_sources.clone();
    if let Some(source) = sources.iter_mut().find(|source| {
        source.scope == Some(InstructionSourceScope::SourceDirectory)
            && source.apply_to.is_empty()
            && source.targets.is_empty()
            && source.exclude.is_empty()
    }) {
        source.files.extend(files);
        source.files.sort();
        source.files.dedup();
    } else {
        sources.push(InstructionSource {
            files,
            exclude: Vec::new(),
            scope: Some(InstructionSourceScope::SourceDirectory),
            apply_to: Vec::new(),
            targets: Vec::new(),
        });
    }
    if sources != original_sources {
        document.set_instruction_sources(&sources);
    }
    let manifest = document.manifest()?;
    let projection = if args.no_sync {
        ProjectionPolicy::LockOnly
    } else {
        ProjectionPolicy::Project(CollisionPolicy::from_flags(args.merge, args.force)?)
    };
    let request = policy
        .request(args.dry_run, projection)
        .with_manifest_bytes(document.bytes());
    super::execute(project, &manifest, request, policy.output)
}

pub(super) fn remove(
    project: &Path,
    args: InstructionRemoveArgs,
    policy: super::ExecutionPolicy,
) -> Result<()> {
    let _guard = super::begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let requested = args.files.into_iter().collect::<BTreeSet<_>>();
    let declared = manifest
        .instructions
        .sources
        .iter()
        .flat_map(|source| source.files.iter().cloned())
        .collect::<BTreeSet<_>>();
    if let Some(missing) = requested.iter().find(|file| !declared.contains(*file)) {
        return Err(AruError::msg(format!(
            "instruction source file selector {missing:?} is not declared"
        )));
    }

    let mut sources = manifest.instructions.sources;
    for source in &mut sources {
        source.files.retain(|file| !requested.contains(file));
    }
    sources.retain(|source| !source.files.is_empty());
    document.set_instruction_sources(&sources);
    let manifest = document.manifest()?;
    let projection = if args.no_sync {
        ProjectionPolicy::LockOnly
    } else {
        ProjectionPolicy::Project(CollisionPolicy::Reject)
    };
    let request = policy
        .request(args.dry_run, projection)
        .with_manifest_bytes(document.bytes());
    super::execute(project, &manifest, request, policy.output)
}

pub(super) fn list(project: &Path) -> Result<()> {
    let manifest = ManifestDocument::load(project)?.manifest()?;
    let mut files = manifest
        .instructions
        .sources
        .into_iter()
        .flat_map(|source| source.files)
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    for file in files {
        println!("{file}");
    }
    Ok(())
}
