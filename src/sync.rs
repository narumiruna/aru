mod plan;
mod request;

pub(crate) use request::{CollisionPolicy, ReconcileRequest, UpdateSelection, prepare_request};

use plan::{lock_details, lock_diff_plan, update_previews};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::cache::Cache;
use crate::digest::sha256_bytes;
use crate::error::{AruError, IoContext, Result};
use crate::instruction;
use crate::lockfile::{LOCK_FILE, Lockfile};
use crate::manifest::{Manifest, Target};
use crate::ownership::{OwnershipAction, STATE_FILE, State, StateEntry, reconcile};
use crate::resolver::{ResolveOptions, SkillResolutionHint, resolve};
use crate::skill::canonical_skill_digest;
use crate::target::{
    self as target_adapter,
    mcp::{self as mcp_adapter, McpConfig},
    skill::{self as skill_adapter, SkillDeploymentMode},
};
use crate::transaction::{Operation, path_digest};

pub struct SyncOptions<'a> {
    pub previous: Option<&'a Lockfile>,
    pub locked: bool,
    pub offline: bool,
    pub materialize_skills: bool,
    pub dry_run: bool,
    pub project_projections: bool,
    pub force: bool,
    pub merge_instructions: bool,
    pub manifest_bytes: Option<Vec<u8>>,
    pub update_skills: &'a BTreeSet<String>,
    pub update_mcp: &'a BTreeSet<String>,
    pub update_packages: &'a BTreeSet<String>,
    pub precise_packages: &'a BTreeMap<String, String>,
    pub skill_hints: &'a BTreeMap<String, SkillResolutionHint>,
}

pub struct SyncResult {
    pub lock: Lockfile,
    pub operations: Vec<Operation>,
    pub plan: Vec<String>,
    pub previews: Vec<String>,
    pub details: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn prepare(
    project: &Path,
    manifest: &Manifest,
    options: SyncOptions<'_>,
) -> Result<SyncResult> {
    let resolution = resolve(
        project,
        manifest,
        ResolveOptions {
            previous: options.previous,
            locked: options.locked,
            offline: options.offline,
            materialize_skills: options.materialize_skills,
            update_skills: options.update_skills,
            update_mcp: options.update_mcp,
            update_packages: options.update_packages,
            precise_packages: options.precise_packages,
            dry_run: options.dry_run,
            skill_hints: options.skill_hints,
        },
    )?;
    let mut operations = Vec::new();
    let mut plan = lock_diff_plan(options.previous, &resolution.lock);
    let mut warnings = Vec::new();

    if options.project_projections {
        prepare_projections(
            project,
            manifest,
            &resolution.lock,
            options.previous,
            &resolution.skill_sources,
            &resolution.instructions,
            options.merge_instructions,
            options.force,
            &mut operations,
            &mut plan,
            &mut warnings,
        )?;
    } else {
        let state = State::load(project)?;
        let state_map = state.by_identity();
        warn_unowned_removed_projections(
            project,
            options.previous,
            &resolution.lock,
            &state_map,
            &mut warnings,
        );
        instruction::sync::warn_unowned_removed(
            project,
            options.previous,
            &resolution.lock,
            &state_map,
            &mut warnings,
        )?;
        validate_deferred_skill_layout(project, options.previous, &resolution.lock, &state_map)?;
    }

    let lock_bytes = resolution.lock.bytes()?;
    push_file_if_changed(
        project,
        LOCK_FILE,
        lock_bytes,
        &mut operations,
        &mut plan,
        "write lockfile",
    )?;
    if let Some(bytes) = options.manifest_bytes {
        push_file_if_changed(
            project,
            crate::manifest::MANIFEST_FILE,
            bytes,
            &mut operations,
            &mut plan,
            "write manifest",
        )?;
    }
    operations.sort_by(|left, right| left.destination.cmp(&right.destination));
    plan.sort();
    warnings.sort();
    warnings.dedup();
    let previews = update_previews(
        options.previous,
        &resolution.lock,
        options.update_skills,
        options.update_mcp,
        options.update_packages,
    );
    let details = lock_details(&resolution.lock);
    Ok(SyncResult {
        lock: resolution.lock,
        operations,
        plan,
        previews,
        details,
        warnings,
    })
}

#[allow(clippy::too_many_arguments)]
fn prepare_projections(
    project: &Path,
    manifest: &Manifest,
    lock: &Lockfile,
    previous: Option<&Lockfile>,
    skill_sources: &BTreeMap<String, PathBuf>,
    instructions: &[instruction::DiscoveredInstruction],
    merge_instructions: bool,
    force: bool,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let state = State::load(project)?;
    let state_map = state.by_identity();
    let lock_identity = lock.lock_identity_digest()?;
    let mut next_state = Vec::new();
    let mut processed = BTreeSet::new();

    for package in &lock.skill_packages {
        for skill in &package.skills {
            let source = skill_sources.get(&skill.name).ok_or_else(|| {
                AruError::msg(format!("missing materialized skill {:?}", skill.name))
            })?;
            let mut destinations = BTreeSet::new();
            for target in &package.targets {
                let layout = skill_adapter::layout(*target, &package.targets, &skill.name)?;
                if !destinations.insert(layout.destination.clone()) {
                    continue;
                }
                prepare_skill_entry(
                    project,
                    lock,
                    previous,
                    &state_map,
                    *target,
                    &skill.name,
                    &skill.sha256,
                    source,
                    layout.destination,
                    layout.mode,
                    layout.link_target,
                    force,
                    &lock_identity,
                    operations,
                    plan,
                    &mut next_state,
                    &mut processed,
                )?;
            }
        }
    }

    warn_unowned_removed_projections(project, previous, lock, &state_map, warnings);

    for entry in &state.entries {
        let identity = state_identity(entry);
        if entry.kind == "skill" && !processed.contains(&identity) {
            let destination = PathBuf::from(&entry.destination);
            let expected_link =
                (entry.mode == "symlink").then(|| skill_adapter::shared_link_target(&entry.key));
            let current = observe_skill(project, &destination, expected_link.as_deref())?;
            match reconcile(
                &entry.key,
                current.as_deref(),
                Some(entry),
                None,
                None,
                force,
            )? {
                OwnershipAction::Remove => {
                    operations.push(Operation::remove(destination.clone()));
                    plan.push(format!(
                        "remove skill {} ({})",
                        entry.key, entry.destination
                    ));
                }
                OwnershipAction::ForgetMissing => {
                    plan.push(format!("forget missing skill {}", entry.key));
                }
                _ => next_state.push(entry.clone()),
            }
            processed.insert(identity);
        }
    }

    instruction::sync::prepare(
        project,
        instructions,
        lock,
        previous,
        &state,
        &state_map,
        merge_instructions,
        force,
        &lock_identity,
        operations,
        plan,
        warnings,
        &mut next_state,
        &mut processed,
    )?;

    prepare_mcp(
        project,
        manifest,
        lock,
        &state,
        &state_map,
        force,
        &lock_identity,
        operations,
        plan,
        &mut next_state,
        &mut processed,
    )?;

    for entry in &state.entries {
        if !processed.contains(&state_identity(entry)) {
            next_state.push(entry.clone());
        }
    }
    let next = State {
        version: 1,
        entries: next_state,
    };
    push_file_if_changed(
        project,
        STATE_FILE,
        next.bytes()?,
        operations,
        plan,
        "write local ownership state",
    )?;
    Ok(())
}

fn projected_skill_mode(lock: &Lockfile, target: Target, name: &str) -> Option<&'static str> {
    let selected_targets = lock
        .projection_baselines
        .iter()
        .filter(|baseline| baseline.kind == "skill" && baseline.key == name)
        .map(|baseline| baseline.target)
        .collect::<Vec<_>>();
    selected_targets
        .contains(&target)
        .then(|| skill_adapter::layout(target, &selected_targets, name).ok())
        .flatten()
        .map(|layout| layout.mode.as_str())
}

fn validate_deferred_skill_layout(
    project: &Path,
    previous: Option<&Lockfile>,
    lock: &Lockfile,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    for baseline in &lock.projection_baselines {
        if baseline.kind != "skill" {
            continue;
        }
        let Some(previous_mode) = projected_skill_mode(previous, baseline.target, &baseline.key)
        else {
            continue;
        };
        let desired_mode = projected_skill_mode(lock, baseline.target, &baseline.key).unwrap();
        if previous_mode == desired_mode {
            continue;
        }
        let destination = skill_adapter::destination(baseline.target, &baseline.key)
            .ok_or_else(|| AruError::msg("internal error: missing skill projection layout"))?;
        let destination = portable(&destination)?;
        let identity = ("skill".into(), baseline.key.clone(), destination.clone());
        if !state_map.contains_key(&identity)
            && std::fs::symlink_metadata(project.join(&destination)).is_ok()
        {
            return Err(AruError::msg(format!(
                "cannot defer the target change with missing local ownership state: {destination} must change from {previous_mode} to {desired_mode}; rerun without --no-sync"
            )));
        }
    }
    Ok(())
}

fn warn_unowned_removed_projections(
    project: &Path,
    previous: Option<&Lockfile>,
    lock: &Lockfile,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
    warnings: &mut Vec<String>,
) {
    let Some(previous) = previous else {
        return;
    };
    for baseline in &previous.projection_baselines {
        let destination = match baseline.kind.as_str() {
            "skill" => skill_adapter::destination(baseline.target, &baseline.key)
                .and_then(|path| portable(&path).ok()),
            "mcp" => mcp_adapter::destination(baseline.target).map(str::to_owned),
            _ => None,
        };
        let Some(destination) = destination else {
            continue;
        };
        let remains_desired = lock.projection_baselines.iter().any(|desired| {
            if desired.kind != baseline.kind || desired.key != baseline.key {
                return false;
            }
            match desired.kind.as_str() {
                "skill" => {
                    skill_adapter::destination(desired.target, &desired.key)
                        .and_then(|path| portable(&path).ok())
                        .as_deref()
                        == Some(destination.as_str())
                }
                "mcp" => mcp_adapter::destination(desired.target) == Some(destination.as_str()),
                _ => false,
            }
        });
        if remains_desired {
            continue;
        }
        let identity = (
            baseline.kind.clone(),
            baseline.key.clone(),
            destination.clone(),
        );
        if state_map.contains_key(&identity)
            || std::fs::symlink_metadata(project.join(&destination)).is_err()
        {
            continue;
        }
        warnings.push(format!(
            "no local ownership record for removed {} {} {:?}; preserved {} for manual review",
            baseline.target, baseline.kind, baseline.key, destination
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_skill_entry(
    project: &Path,
    lock: &Lockfile,
    previous: Option<&Lockfile>,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
    target: Target,
    name: &str,
    desired_digest: &str,
    source: &Path,
    destination: PathBuf,
    mode: SkillDeploymentMode,
    link_target: Option<PathBuf>,
    force: bool,
    lock_identity: &str,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    next_state: &mut Vec<StateEntry>,
    processed: &mut BTreeSet<(String, String, String)>,
) -> Result<()> {
    let destination_string = portable(&destination)?;
    let identity = ("skill".into(), name.into(), destination_string.clone());
    let owned = state_map.get(&identity).copied();
    let baseline = lock
        .projection_baselines
        .iter()
        .find(|baseline| {
            baseline.target == target && baseline.kind == "skill" && baseline.key == name
        })
        .map(|baseline| baseline.sha256.as_str());
    let previous_mode = previous.and_then(|previous| projected_skill_mode(previous, target, name));
    let observed_mode = owned
        .map(|entry| entry.mode.as_str())
        .or(previous_mode)
        .unwrap_or(mode.as_str());
    let observed_link =
        (observed_mode == "symlink").then(|| skill_adapter::shared_link_target(name));
    let current = observe_skill(project, &destination, observed_link.as_deref())?;
    let mut action = reconcile(
        name,
        current.as_deref(),
        owned,
        baseline,
        Some(desired_digest),
        force,
    )?;
    let mode_changed =
        (owned.is_some() || previous_mode.is_some()) && observed_mode != mode.as_str();
    if mode_changed && matches!(action, OwnershipAction::Adopt | OwnershipAction::Noop) {
        action = OwnershipAction::Update;
    }
    match action {
        OwnershipAction::Create | OwnershipAction::Update => {
            if mode == SkillDeploymentMode::Symlink {
                operations.push(Operation::symlink(
                    destination.clone(),
                    link_target
                        .clone()
                        .expect("symlink layout has a link target"),
                ));
            } else {
                operations.push(Operation::skill_directory(
                    destination.clone(),
                    source,
                    desired_digest,
                ));
            }
            let verb = if action == OwnershipAction::Create {
                "create"
            } else if force && owned.is_none() {
                "force replace"
            } else {
                "update"
            };
            plan.push(format!("{verb} skill {name} ({destination_string})"));
        }
        OwnershipAction::Adopt => plan.push(format!("adopt skill {name} ({destination_string})")),
        OwnershipAction::Noop => {}
        _ => return Err(AruError::msg("unexpected skill ownership action")),
    }
    next_state.push(StateEntry {
        destination: destination_string,
        kind: "skill".into(),
        key: name.into(),
        mode: mode.as_str().into(),
        last_applied_digest: desired_digest.into(),
        lock_identity: lock_identity.into(),
    });
    processed.insert(identity);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prepare_mcp(
    project: &Path,
    manifest: &Manifest,
    lock: &Lockfile,
    state: &State,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
    force: bool,
    lock_identity: &str,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    next_state: &mut Vec<StateEntry>,
    processed: &mut BTreeSet<(String, String, String)>,
) -> Result<()> {
    let needed_targets = manifest
        .project
        .targets
        .iter()
        .copied()
        .filter(|target| target_adapter::capabilities(*target).mcp)
        .chain(state.entries.iter().filter_map(|entry| {
            (entry.kind == "mcp")
                .then(|| mcp_adapter::target_for_destination(&entry.destination))
                .flatten()
        }))
        .collect::<BTreeSet<_>>();
    let mut configs = needed_targets
        .into_iter()
        .map(|target| McpConfig::load(project, target).map(|config| (target, config)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut changed = BTreeSet::new();

    for server in &lock.mcp_servers {
        for target in &server.targets {
            let destination = mcp_adapter::destination(target.target).ok_or_else(|| {
                AruError::msg(format!(
                    "internal error: MCP projection reached unsupported target {}",
                    target.target
                ))
            })?;
            let identity = ("mcp".into(), server.name.clone(), destination.into());
            let owned = state_map.get(&identity).copied();
            let desired = target_adapter::entry_digest(target)?;
            let baseline = lock
                .projection_baselines
                .iter()
                .find(|baseline| {
                    baseline.target == target.target
                        && baseline.kind == "mcp"
                        && baseline.key == server.name
                })
                .map(|baseline| baseline.sha256.as_str());
            let config = configs.get_mut(&target.target).ok_or_else(|| {
                AruError::msg("internal error: missing MCP configuration adapter")
            })?;
            let current = config.digest(&server.name)?;
            let action = reconcile(
                &server.name,
                current.as_deref(),
                owned,
                baseline,
                Some(&desired),
                force,
            )?;
            match action {
                OwnershipAction::Create | OwnershipAction::Update => {
                    config.set(&server.name, target)?;
                    changed.insert(target.target);
                    let verb = if action == OwnershipAction::Create {
                        "create"
                    } else if force && owned.is_none() {
                        "force replace"
                    } else {
                        "update"
                    };
                    plan.push(format!("{verb} MCP {} ({destination})", server.name));
                }
                OwnershipAction::Adopt => {
                    plan.push(format!("adopt MCP {} ({destination})", server.name));
                }
                OwnershipAction::Noop => {}
                _ => return Err(AruError::msg("unexpected MCP ownership action")),
            }
            next_state.push(StateEntry {
                destination: destination.into(),
                kind: "mcp".into(),
                key: server.name.clone(),
                mode: "merge".into(),
                last_applied_digest: desired,
                lock_identity: lock_identity.into(),
            });
            processed.insert(identity);
        }
    }

    for entry in &state.entries {
        let identity = state_identity(entry);
        if entry.kind != "mcp" || processed.contains(&identity) {
            continue;
        }
        let Some(target) = mcp_adapter::target_for_destination(&entry.destination) else {
            next_state.push(entry.clone());
            processed.insert(identity);
            continue;
        };
        let config = configs
            .get_mut(&target)
            .ok_or_else(|| AruError::msg("internal error: missing MCP configuration adapter"))?;
        let current = config.digest(&entry.key)?;
        match reconcile(
            &entry.key,
            current.as_deref(),
            Some(entry),
            None,
            None,
            force,
        )? {
            OwnershipAction::Remove => {
                config.remove(&entry.key);
                changed.insert(target);
                plan.push(format!("remove MCP {} ({})", entry.key, entry.destination));
            }
            OwnershipAction::ForgetMissing => {
                plan.push(format!("forget missing MCP {}", entry.key));
            }
            _ => next_state.push(entry.clone()),
        }
        processed.insert(identity);
    }

    for target in changed {
        let destination = mcp_adapter::destination(target)
            .expect("changed MCP configurations have supported targets");
        operations.push(Operation::file(destination, configs[&target].bytes()?));
    }
    Ok(())
}

fn observe_skill(
    project: &Path,
    relative: &Path,
    expected_link: Option<&Path>,
) -> Result<Option<String>> {
    let path = project.join(relative);
    let Ok(metadata) = std::fs::symlink_metadata(&path) else {
        return Ok(None);
    };
    if metadata.file_type().is_symlink() {
        let actual = std::fs::read_link(&path).at(&path)?;
        if expected_link != Some(actual.as_path()) {
            return Ok(Some(format!(
                "unexpected-link:{}",
                sha256_bytes(actual.to_string_lossy().as_bytes())
            )));
        }
        let resolved = path
            .parent()
            .unwrap()
            .join(&actual)
            .canonicalize()
            .at(&path)?;
        let project_root = project.canonicalize().at(project)?;
        if !resolved.starts_with(project_root) {
            return Ok(Some("escaping-link".into()));
        }
        return canonical_skill_digest(&resolved).map(Some);
    }
    if metadata.is_dir() {
        return canonical_skill_digest(&path).map(Some);
    }
    Ok(path_digest(&path)?.map(|digest| format!("wrong-type:{digest}")))
}

fn push_file_if_changed(
    project: &Path,
    destination: &str,
    bytes: Vec<u8>,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    description: &str,
) -> Result<()> {
    let path = project.join(destination);
    if path.exists() && std::fs::read(&path).at(&path)? == bytes {
        return Ok(());
    }
    operations.push(Operation::file(destination, bytes));
    plan.push(description.into());
    Ok(())
}

fn state_identity(entry: &StateEntry) -> (String, String, String) {
    (
        entry.kind.clone(),
        entry.key.clone(),
        entry.destination.clone(),
    )
}

fn portable(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| AruError::msg("projection path is not UTF-8"))
}

pub fn garbage_collect(project: &Path, lock: &Lockfile) -> Result<()> {
    let referenced = lock
        .skill_packages
        .iter()
        .map(|package| (package.source.clone(), package.revision.clone()))
        .chain(
            lock.aru_packages
                .iter()
                .map(|package| (package.source.clone(), package.revision.clone())),
        )
        .collect::<Vec<_>>();
    Cache::project(project).garbage_collect(&referenced)
}
