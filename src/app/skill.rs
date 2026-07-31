use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cli::{SkillAddArgs, SkillRemoveArgs, SkillUpdateArgs};
use crate::error::{AruError, IoContext, Result};
use crate::interactive::{
    InquireSkillChooser, SkillAddSelectionMode, SkillChooser, choose_skills,
    terminal_selection_mode,
};
use crate::lockfile::Lockfile;
use crate::manifest::{ManifestDocument, SkillRequirement, validate_name};
use crate::resolver::{canonical_update_skill_targets, inspect_skill_source};

use super::{ExecutionPolicy, begin, execute, execute_with_skill_hints};

pub(super) fn add(project: &Path, args: SkillAddArgs, policy: ExecutionPolicy) -> Result<()> {
    let mode = terminal_selection_mode(args.all, !args.skills.is_empty(), args.path.is_some())?;
    let mut chooser = InquireSkillChooser;
    skill_add_with_policy(project, args, mode, &mut chooser, policy)
}

#[cfg(test)]
pub(super) fn skill_add_with_mode(
    project: &Path,
    args: SkillAddArgs,
    mode: SkillAddSelectionMode,
    chooser: &mut dyn SkillChooser,
) -> Result<()> {
    skill_add_with_policy(project, args, mode, chooser, ExecutionPolicy::default())
}

fn skill_add_with_policy(
    project: &Path,
    args: SkillAddArgs,
    mode: SkillAddSelectionMode,
    chooser: &mut dyn SkillChooser,
    policy: ExecutionPolicy,
) -> Result<()> {
    if mode != SkillAddSelectionMode::Interactive {
        return skill_add_explicit(project, &args, mode, policy);
    }

    let (snapshot, key, requirement, existing, previous) = {
        let _guard = begin(project, args.dry_run)?;
        let snapshot = ProjectSnapshot::read(project)?;
        let document = ManifestDocument::load(project)?;
        let manifest = document.manifest()?;
        let key = find_skill_key(project, &manifest, &args.source)?.unwrap_or(args.source.clone());
        let existing = manifest.skills.get(&key).cloned();
        let requirement = skill_add_base_requirement(existing.as_ref(), &args);
        requirement.validate(&key)?;
        requirement.validate_targets(&key, &manifest.project.targets)?;
        let previous = Lockfile::load_optional(project)?;
        (snapshot, key, requirement, existing, previous)
    };

    policy.output.progress(&format!("skill source {key}"));
    let inspection = inspect_skill_source(
        project,
        &key,
        &requirement,
        if args.upgrade {
            None
        } else {
            previous.as_ref()
        },
        args.dry_run,
        policy.offline,
    )?;
    let names = inspection
        .candidates
        .iter()
        .map(|candidate| candidate.name.clone())
        .collect::<Vec<_>>();
    let current = match existing.as_ref() {
        None => Vec::new(),
        Some(requirement) if requirement.is_wildcard() => names
            .iter()
            .filter(|name| !requirement.exclude.contains(name))
            .cloned()
            .collect(),
        Some(requirement) => requirement.include.clone(),
    };
    let Some(selected) = choose_skills(chooser, &names, &current)? else {
        policy
            .output
            .completion("Skill selection canceled; no files were changed.");
        return Ok(());
    };

    let _guard = begin(project, args.dry_run)?;
    if ProjectSnapshot::read(project)? != snapshot {
        return Err(AruError::msg(
            "aru.toml or aru.lock changed during interactive skill selection; retry the command",
        ));
    }
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let current_key =
        find_skill_key(project, &manifest, &args.source)?.unwrap_or(args.source.clone());
    if current_key != key {
        return Err(AruError::msg(
            "skill source identity changed during interactive skill selection; retry the command",
        ));
    }
    let current_existing = manifest.skills.get(&key);
    let mut selected_requirement = skill_add_base_requirement(current_existing, &args);
    let preserve_wildcard = current_existing.is_some_and(SkillRequirement::is_wildcard)
        && selected.len() == names.len()
        && selected.iter().all(|name| names.contains(name));
    if preserve_wildcard {
        selected_requirement.include = vec!["*".into()];
        selected_requirement.exclude.clear();
    } else {
        selected_requirement.include = selected.clone();
        selected_requirement.exclude.clear();
    }
    selected_requirement
        .paths
        .retain(|name, _| selected.contains(name));
    selected_requirement.normalize();
    selected_requirement.validate(&key)?;
    document.set_skill(&key, &selected_requirement);
    let manifest = document.manifest()?;
    let hints = BTreeMap::from([(inspection.source.clone(), inspection.hint())]);
    let updates = skill_add_update_targets(project, &manifest, &key, args.upgrade)?;
    execute_with_skill_hints(
        project,
        &manifest,
        Some(document.bytes()),
        args.dry_run,
        policy,
        !args.no_sync,
        false,
        args.force,
        updates,
        BTreeSet::new(),
        &hints,
    )
}

fn skill_add_explicit(
    project: &Path,
    args: &SkillAddArgs,
    mode: SkillAddSelectionMode,
    policy: ExecutionPolicy,
) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let key = find_skill_key(project, &manifest, &args.source)?.unwrap_or(args.source.clone());
    let existing = manifest.skills.get(&key);
    let mut requirement = skill_add_base_requirement(existing, args);
    if existing.is_none() && mode == SkillAddSelectionMode::Explicit {
        requirement.include.clear();
    }
    if mode == SkillAddSelectionMode::All {
        requirement.include = vec!["*".into()];
        requirement.exclude.clear();
    } else {
        for name in &args.skills {
            validate_name(name, "skill name")?;
            add_skill_selector(&mut requirement, name);
        }
        if let Some(path) = &args.path {
            let parsed = crate::skill::validate_relative_selector(path)?;
            let name = parsed
                .file_name()
                .and_then(|part| part.to_str())
                .ok_or_else(|| AruError::msg("skill path has no UTF-8 directory name"))?
                .to_owned();
            validate_name(&name, "skill name")?;
            add_skill_selector(&mut requirement, &name);
            requirement.paths.insert(name, path.clone());
        }
    }
    requirement.normalize();
    requirement.validate(&key)?;
    document.set_skill(&key, &requirement);
    let manifest = document.manifest()?;
    let updates = skill_add_update_targets(project, &manifest, &key, args.upgrade)?;
    execute(
        project,
        &manifest,
        Some(document.bytes()),
        args.dry_run,
        policy,
        !args.no_sync,
        false,
        args.force,
        updates,
        BTreeSet::new(),
    )
}

fn skill_add_update_targets(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    key: &str,
    upgrade: bool,
) -> Result<BTreeSet<String>> {
    if upgrade {
        canonical_update_skill_targets(project, manifest, &[key.to_owned()])
    } else {
        Ok(BTreeSet::new())
    }
}

fn skill_add_base_requirement(
    existing: Option<&SkillRequirement>,
    args: &SkillAddArgs,
) -> SkillRequirement {
    let mut requirement = existing.cloned().unwrap_or_default();
    if let Some(version) = &args.version {
        requirement.version = Some(version.clone());
        requirement.branch = None;
        requirement.rev = None;
    }
    if let Some(branch) = &args.branch {
        requirement.branch = Some(branch.clone());
        requirement.version = None;
        requirement.rev = None;
    }
    if let Some(revision) = &args.rev {
        requirement.rev = Some(revision.clone());
        requirement.version = None;
        requirement.branch = None;
    }
    if !args.targets.is_empty() {
        requirement.targets = Some(args.targets.clone());
    }
    requirement
}

#[derive(Debug, PartialEq, Eq)]
struct ProjectSnapshot {
    manifest: Vec<u8>,
    lock: Option<Vec<u8>>,
}

impl ProjectSnapshot {
    fn read(project: &Path) -> Result<Self> {
        let manifest_path = project.join(crate::manifest::MANIFEST_FILE);
        let lock_path = project.join(crate::lockfile::LOCK_FILE);
        let manifest = std::fs::read(&manifest_path).at(&manifest_path)?;
        let lock = if lock_path.exists() {
            Some(std::fs::read(&lock_path).at(&lock_path)?)
        } else {
            None
        };
        Ok(Self { manifest, lock })
    }
}

pub(super) fn remove(project: &Path, args: SkillRemoveArgs, policy: ExecutionPolicy) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let key = find_skill_key(project, &manifest, &args.source)?
        .ok_or_else(|| AruError::msg(format!("skill source {:?} is not declared", args.source)))?;
    if args.skills.is_empty() {
        document.remove_skill(&key);
    } else {
        let mut requirement = manifest.skills.get(&key).unwrap().clone();
        for name in args.skills {
            validate_name(&name, "skill name")?;
            if requirement.is_wildcard() {
                if !requirement.exclude.contains(&name) {
                    requirement.exclude.push(name.clone());
                }
            } else if let Some(index) = requirement.include.iter().position(|item| item == &name) {
                requirement.include.remove(index);
            } else {
                return Err(AruError::msg(format!(
                    "skill {name:?} is not explicitly selected from {key:?}"
                )));
            }
            requirement.paths.remove(&name);
        }
        requirement.normalize();
        if requirement.include.is_empty() {
            document.remove_skill(&key);
        } else {
            document.set_skill(&key, &requirement);
        }
    }
    let manifest = document.manifest()?;
    execute(
        project,
        &manifest,
        Some(document.bytes()),
        args.dry_run,
        policy,
        !args.no_sync,
        false,
        false,
        BTreeSet::new(),
        BTreeSet::new(),
    )
}

pub(super) fn list(project: &Path) -> Result<()> {
    let manifest = ManifestDocument::load(project)?.manifest()?;
    let lock = Lockfile::load_optional(project)?;
    let mut listed_sources = BTreeSet::new();
    if let Some(lock) = lock {
        for package in lock.skill_packages {
            listed_sources.insert(package.source.clone());
            for skill in package.skills {
                println!("{}\t{}\t{}", skill.name, package.version, package.source);
            }
        }
    }
    for source in manifest.skills.keys() {
        let canonical = crate::source::git::canonicalize(project, source)?;
        if !listed_sources.contains(&canonical.identity) {
            println!("-\tunlocked\t{source}");
        }
    }
    Ok(())
}

pub(super) fn update(project: &Path, args: SkillUpdateArgs, policy: ExecutionPolicy) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let updates = canonical_update_skill_targets(project, &manifest, &args.sources)?;
    execute(
        project,
        &manifest,
        None,
        args.dry_run,
        policy,
        !args.no_sync,
        false,
        args.force,
        updates,
        BTreeSet::new(),
    )
}

fn add_skill_selector(requirement: &mut SkillRequirement, name: &str) {
    if requirement.is_wildcard() {
        requirement.exclude.retain(|excluded| excluded != name);
    } else if !requirement.include.iter().any(|selected| selected == name) {
        requirement.include.push(name.into());
    }
}

fn find_skill_key(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    requested: &str,
) -> Result<Option<String>> {
    if manifest.skills.contains_key(requested) {
        return Ok(Some(requested.into()));
    }
    let canonical = crate::source::git::canonicalize(project, requested)?;
    for key in manifest.skills.keys() {
        if crate::source::git::canonicalize(project, key)?.identity == canonical.identity {
            return Ok(Some(key.clone()));
        }
    }
    Ok(None)
}
