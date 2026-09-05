use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cache::Cache;
use crate::cli::{SkillAddArgs, SkillRemoveArgs, SkillTargetArg, SkillUpdateArgs};
use crate::error::{AruError, IoContext, Result};
use crate::interactive::{
    InquireSkillChooser, InquireTargetChooser, SkillAddSelectionMode, SkillChooser, TargetChoice,
    choose_skills, terminal_choose_targets, terminal_selection_mode,
};
use crate::lockfile::Lockfile;
use crate::manifest::{ManifestDocument, SkillRequirement, validate_name};
use crate::resolver::{
    canonical_update_skill_targets, declared_skill_source_key, inspect_skill_source,
    inspect_skill_source_with_cache,
};
use crate::skill::select_candidates;
use crate::sync::{CollisionPolicy, UpdateSelection};
use crate::transaction::{Operation, StandaloneDryRun, apply_standalone, apply_standalone_global};

use super::{ExecutionPolicy, ProjectionPolicy, begin, execute};

pub(super) fn add(project: &Path, args: SkillAddArgs, policy: ExecutionPolicy) -> Result<()> {
    if args.global {
        return Err(AruError::msg(
            "--global is only supported for standalone skill installation without aru.toml",
        ));
    }
    let mode = terminal_selection_mode(args.all, !args.skills.is_empty(), args.path.is_some())?;
    let mut chooser = InquireSkillChooser;
    skill_add_with_policy(project, args, mode, &mut chooser, policy)
}

pub(super) fn add_standalone(
    project: &Path,
    mut args: SkillAddArgs,
    policy: ExecutionPolicy,
) -> Result<()> {
    if args.no_sync {
        return Err(AruError::msg(
            "--no-sync requires an initialized aru project; standalone skill installation writes directly to target paths",
        ));
    }
    if policy.locked {
        return Err(AruError::msg(
            "--locked and --frozen require an initialized aru project with aru.lock",
        ));
    }
    if args.targets.is_empty() {
        let mut chooser = InquireTargetChooser;
        let mut choices = Vec::new();
        for spec in crate::target::specs()
            .iter()
            .filter(|spec| spec.capabilities.skills)
        {
            if args.global {
                if let Some(destination) = crate::target::skill::global_directory(spec.target)? {
                    choices.push(TargetChoice::new(
                        spec.target,
                        &destination.display().to_string(),
                    ));
                }
            } else {
                choices.push(TargetChoice::new(spec.target, spec.project_skills));
            }
        }
        let Some(targets) = terminal_choose_targets(&mut chooser, &choices)? else {
            policy
                .output
                .completion("Target selection canceled; no files were changed.");
            return Ok(());
        };
        args.targets = targets.into_iter().map(SkillTargetArg::canonical).collect();
    }
    validate_standalone_targets(&args.targets, args.global)?;
    let mode = terminal_selection_mode(args.all, !args.skills.is_empty(), args.path.is_some())?;
    let mut chooser = InquireSkillChooser;
    standalone_add_with_policy(project, &args, mode, &mut chooser, policy)
}

fn standalone_add_with_policy(
    project: &Path,
    args: &SkillAddArgs,
    mode: SkillAddSelectionMode,
    chooser: &mut dyn SkillChooser,
    policy: ExecutionPolicy,
) -> Result<()> {
    let mut requirement = standalone_requirement(args, mode)?;
    let cache = Cache::ephemeral()?;
    policy
        .output
        .progress(&format!("skill source {}", args.source));
    let inspection = inspect_skill_source_with_cache(
        project,
        &args.source,
        &requirement,
        None,
        policy.offline,
        &cache,
    )?;
    if mode == SkillAddSelectionMode::Interactive {
        let names = inspection
            .candidates
            .iter()
            .map(|candidate| candidate.name.clone())
            .collect::<Vec<_>>();
        let Some(selected) = choose_skills(chooser, &names, &[])? else {
            policy
                .output
                .completion("Skill selection canceled; no files were changed.");
            return Ok(());
        };
        requirement.include = selected;
        requirement.exclude.clear();
        requirement.normalize();
    }
    let selected = select_candidates(inspection.candidates, &requirement)?;
    let dry_run = args
        .dry_run
        .then(|| StandaloneDryRun::begin(project, args.global))
        .transpose()?;
    let mut destinations = BTreeSet::new();
    let mut operations = Vec::new();
    let mut plan = Vec::new();
    let mut collision = None;
    for skill in selected {
        for target_arg in &args.targets {
            let target = target_arg.target;
            let destination = if args.global {
                crate::target::skill::global_directory_for_input(target, &target_arg.requested)?
                    .ok_or_else(|| {
                        AruError::msg(format!(
                            "target {target} does not support global Agent Skills installation"
                        ))
                    })?
                    .join(&skill.name)
            } else {
                crate::target::skill::destination(target, &skill.name).ok_or_else(|| {
                    AruError::msg(format!("target {target} does not support skills"))
                })?
            };
            let absolute_destination = if args.global {
                destination.clone()
            } else {
                project.join(&destination)
            };
            if !destinations.insert(absolute_destination.clone()) {
                continue;
            }
            let exists = standalone_destination_exists(&absolute_destination)?;
            if exists && !args.force && args.dry_run && collision.is_none() {
                collision = Some(format!(
                    "collision: unmanaged skill {:?} already exists at {}; inspect it or rerun with --force",
                    skill.name,
                    destination.display()
                ));
            }
            let verb = if exists { "force replace" } else { "create" };
            plan.push(format!(
                "{verb} skill {} ({})",
                skill.name,
                destination.display()
            ));
            operations.push(Operation::skill_directory(
                destination,
                &skill.absolute_path,
                &skill.sha256,
            ));
        }
    }
    plan.sort();
    if let Some(dry_run) = dry_run {
        dry_run.validate(&operations)?;
        if let Some(collision) = collision {
            return Err(AruError::msg(collision));
        }
        for item in &plan {
            policy.output.plan(item, true);
        }
        policy
            .output
            .completion("Dry run complete; no files were changed.");
        return Ok(());
    }
    if args.global {
        apply_standalone_global(project, operations, args.force)?;
    } else {
        apply_standalone(project, operations, args.force)?;
    }
    for item in &plan {
        policy.output.plan(item, false);
    }
    let completion = if args.global {
        "Global skills installed; no aru project state was created."
    } else {
        "Standalone skills installed; no aru project state was created."
    };
    policy.output.completion(completion);
    Ok(())
}

fn validate_standalone_targets(targets: &[SkillTargetArg], global: bool) -> Result<()> {
    if targets.is_empty() {
        return Err(AruError::msg(
            "standalone skill installation requires a target",
        ));
    }
    let mut identities = BTreeSet::new();
    for target_arg in targets {
        let target = target_arg.target;
        if !crate::target::capabilities(target).skills {
            return Err(AruError::msg(format!(
                "target {target} does not support Agent Skills"
            )));
        }
        let identity = if global {
            let directory =
                crate::target::skill::global_directory_for_input(target, &target_arg.requested)?
                    .ok_or_else(|| {
                        AruError::msg(format!(
                            "target {target} does not support global Agent Skills installation"
                        ))
                    })?;
            (target, Some(directory))
        } else {
            (target, None)
        };
        if !identities.insert(identity) {
            return Err(AruError::msg(
                "skill dependency targets contains duplicates",
            ));
        }
    }
    Ok(())
}

fn standalone_requirement(
    args: &SkillAddArgs,
    mode: SkillAddSelectionMode,
) -> Result<SkillRequirement> {
    let mut requirement = skill_add_base_requirement(None, args);
    requirement.targets = None;
    if mode == SkillAddSelectionMode::Explicit {
        requirement.include.clear();
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
    } else if mode == SkillAddSelectionMode::All {
        requirement.include = vec!["*".into()];
        requirement.exclude.clear();
    }
    requirement.normalize();
    requirement.validate(&args.source)?;
    Ok(requirement)
}

fn standalone_destination_exists(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AruError::msg(format!(
            "could not inspect {}: {error}",
            path.display()
        ))),
    }
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
        let key = declared_skill_source_key(project, &manifest, &args.source)?
            .unwrap_or(args.source.clone());
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
        declared_skill_source_key(project, &manifest, &args.source)?.unwrap_or(args.source.clone());
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
    let projection = skill_projection(args.no_sync, args.force);
    let request = policy
        .request(args.dry_run, projection)
        .with_manifest_bytes(document.bytes())
        .with_updates(
            UpdateSelection::default()
                .skills(updates)
                .skill_hints(hints),
        );
    execute(project, &manifest, request, policy.output)
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
    let key =
        declared_skill_source_key(project, &manifest, &args.source)?.unwrap_or(args.source.clone());
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
    let projection = skill_projection(args.no_sync, args.force);
    let request = policy
        .request(args.dry_run, projection)
        .with_manifest_bytes(document.bytes())
        .with_updates(UpdateSelection::default().skills(updates));
    execute(project, &manifest, request, policy.output)
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
        requirement.targets = Some(args.targets.iter().map(|target| target.target).collect());
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
    let key = declared_skill_source_key(project, &manifest, &args.source)?
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
    let request = policy
        .request(args.dry_run, skill_projection(args.no_sync, false))
        .with_manifest_bytes(document.bytes());
    execute(project, &manifest, request, policy.output)
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
    let request = policy
        .request(args.dry_run, skill_projection(args.no_sync, args.force))
        .with_updates(UpdateSelection::default().skills(updates));
    execute(project, &manifest, request, policy.output)
}

fn skill_projection(no_sync: bool, force: bool) -> ProjectionPolicy {
    if no_sync {
        ProjectionPolicy::LockOnly
    } else {
        ProjectionPolicy::Project(if force {
            CollisionPolicy::Force
        } else {
            CollisionPolicy::Reject
        })
    }
}

fn add_skill_selector(requirement: &mut SkillRequirement, name: &str) {
    if requirement.is_wildcard() {
        requirement.exclude.retain(|excluded| excluded != name);
    } else if !requirement.include.iter().any(|selected| selected == name) {
        requirement.include.push(name.into());
    }
}
