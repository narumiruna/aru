use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::cache::Cache;
use crate::digest::sha256_bytes;
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::{LOCK_FILE, Lockfile};
use crate::manifest::{Manifest, Target};
use crate::ownership::{OwnershipAction, STATE_FILE, State, StateEntry, reconcile};
use crate::resolver::{ResolveOptions, SkillResolutionHint, resolve};
use crate::skill::canonical_skill_digest;
use crate::target::{self as target_adapter, claude::ClaudeConfig, codex::CodexConfig};
use crate::transaction::{Operation, path_digest};

pub struct SyncOptions<'a> {
    pub previous: Option<&'a Lockfile>,
    pub locked: bool,
    pub dry_run: bool,
    pub project_projections: bool,
    pub force: bool,
    pub manifest_bytes: Option<Vec<u8>>,
    pub update_skills: &'a BTreeSet<String>,
    pub update_mcp: &'a BTreeSet<String>,
    pub skill_hints: &'a BTreeMap<String, SkillResolutionHint>,
}

pub struct SyncResult {
    pub lock: Lockfile,
    pub operations: Vec<Operation>,
    pub plan: Vec<String>,
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
            update_skills: options.update_skills,
            update_mcp: options.update_mcp,
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
    Ok(SyncResult {
        lock: resolution.lock,
        operations,
        plan,
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
    force: bool,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let state = State::load(project)?;
    let state_map = state.by_identity();
    let lock_identity = lock.package_identity_digest()?;
    let mut next_state = Vec::new();
    let mut processed = BTreeSet::new();

    let projects_codex = manifest.project.targets.contains(&Target::Codex);
    let projects_claude = manifest.project.targets.contains(&Target::Claude);
    for package in &lock.skill_packages {
        for skill in &package.skills {
            let source = skill_sources.get(&skill.name).ok_or_else(|| {
                AruError::msg(format!("missing materialized skill {:?}", skill.name))
            })?;
            if projects_codex {
                prepare_skill_entry(
                    project,
                    lock,
                    previous,
                    &state_map,
                    Target::Codex,
                    &skill.name,
                    &skill.sha256,
                    source,
                    PathBuf::from(format!(".agents/skills/{}", skill.name)),
                    "copy",
                    None,
                    force,
                    &lock_identity,
                    operations,
                    plan,
                    &mut next_state,
                    &mut processed,
                )?;
            }
            if projects_claude {
                let destination = PathBuf::from(format!(".claude/skills/{}", skill.name));
                let link_target = (projects_codex && supports_project_symlink())
                    .then(|| PathBuf::from(format!("../../.agents/skills/{}", skill.name)));
                let mode = if link_target.is_some() {
                    "symlink"
                } else {
                    "copy"
                };
                prepare_skill_entry(
                    project,
                    lock,
                    previous,
                    &state_map,
                    Target::Claude,
                    &skill.name,
                    &skill.sha256,
                    source,
                    destination,
                    mode,
                    link_target,
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
            let expected_link = if entry.mode == "symlink" {
                Some(PathBuf::from(format!("../../.agents/skills/{}", entry.key)))
            } else {
                None
            };
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
    lock.projection_baselines
        .iter()
        .any(|baseline| {
            baseline.target == target && baseline.kind == "skill" && baseline.key == name
        })
        .then(|| {
            if target == Target::Claude
                && supports_project_symlink()
                && lock.projection_baselines.iter().any(|baseline| {
                    baseline.target == Target::Codex
                        && baseline.kind == "skill"
                        && baseline.key == name
                })
            {
                "symlink"
            } else {
                "copy"
            }
        })
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
        if baseline.target != Target::Claude || baseline.kind != "skill" {
            continue;
        }
        let Some(previous_mode) = projected_skill_mode(previous, Target::Claude, &baseline.key)
        else {
            continue;
        };
        let desired_mode = projected_skill_mode(lock, Target::Claude, &baseline.key).unwrap();
        if previous_mode == desired_mode {
            continue;
        }
        let destination = format!(".claude/skills/{}", baseline.key);
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
        let remains_desired = lock.projection_baselines.iter().any(|desired| {
            desired.target == baseline.target
                && desired.kind == baseline.kind
                && desired.key == baseline.key
        });
        if remains_desired {
            continue;
        }
        let destination = match (baseline.target, baseline.kind.as_str()) {
            (Target::Codex, "skill") => format!(".agents/skills/{}", baseline.key),
            (Target::Claude, "skill") => format!(".claude/skills/{}", baseline.key),
            (Target::Codex, "mcp") => crate::target::codex::CONFIG_PATH.into(),
            (Target::Claude, "mcp") => crate::target::claude::CONFIG_PATH.into(),
            _ => continue,
        };
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
    mode: &str,
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
        .unwrap_or(mode);
    let observed_link =
        (observed_mode == "symlink").then(|| PathBuf::from(format!("../../.agents/skills/{name}")));
    let current = observe_skill(project, &destination, observed_link.as_deref())?;
    let mut action = reconcile(
        name,
        current.as_deref(),
        owned,
        baseline,
        Some(desired_digest),
        force,
    )?;
    let mode_changed = (owned.is_some() || previous_mode.is_some()) && observed_mode != mode;
    if mode_changed && matches!(action, OwnershipAction::Adopt | OwnershipAction::Noop) {
        action = OwnershipAction::Update;
    }
    match action {
        OwnershipAction::Create | OwnershipAction::Update => {
            if mode == "symlink" {
                operations.push(Operation::symlink(
                    destination.clone(),
                    link_target.clone().unwrap(),
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
        mode: mode.into(),
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
    let needs_codex = manifest.project.targets.contains(&Target::Codex)
        || state.entries.iter().any(|entry| {
            entry.kind == "mcp" && entry.destination == crate::target::codex::CONFIG_PATH
        });
    let needs_claude = manifest.project.targets.contains(&Target::Claude)
        || state.entries.iter().any(|entry| {
            entry.kind == "mcp" && entry.destination == crate::target::claude::CONFIG_PATH
        });
    let mut codex = needs_codex
        .then(|| CodexConfig::load(project))
        .transpose()?;
    let mut claude = needs_claude
        .then(|| ClaudeConfig::load(project))
        .transpose()?;
    let mut codex_changed = false;
    let mut claude_changed = false;

    for server in &lock.mcp_servers {
        for target in &server.targets {
            let destination = match target.target {
                Target::Codex => crate::target::codex::CONFIG_PATH,
                Target::Claude => crate::target::claude::CONFIG_PATH,
            };
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
            let current = match target.target {
                Target::Codex => codex.as_ref().unwrap().digest(&server.name)?,
                Target::Claude => claude.as_ref().unwrap().digest(&server.name)?,
            };
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
                    match target.target {
                        Target::Codex => {
                            codex.as_mut().unwrap().set(&server.name, target)?;
                            codex_changed = true;
                        }
                        Target::Claude => {
                            claude.as_mut().unwrap().set(&server.name, target)?;
                            claude_changed = true;
                        }
                    }
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
        let current = match entry.destination.as_str() {
            crate::target::codex::CONFIG_PATH => codex.as_ref().unwrap().digest(&entry.key)?,
            crate::target::claude::CONFIG_PATH => claude.as_ref().unwrap().digest(&entry.key)?,
            _ => {
                next_state.push(entry.clone());
                processed.insert(identity);
                continue;
            }
        };
        match reconcile(
            &entry.key,
            current.as_deref(),
            Some(entry),
            None,
            None,
            force,
        )? {
            OwnershipAction::Remove => {
                match entry.destination.as_str() {
                    crate::target::codex::CONFIG_PATH => {
                        codex.as_mut().unwrap().remove(&entry.key);
                        codex_changed = true;
                    }
                    crate::target::claude::CONFIG_PATH => {
                        claude.as_mut().unwrap().remove(&entry.key);
                        claude_changed = true;
                    }
                    _ => unreachable!(),
                }
                plan.push(format!("remove MCP {} ({})", entry.key, entry.destination));
            }
            OwnershipAction::ForgetMissing => {
                plan.push(format!("forget missing MCP {}", entry.key));
            }
            _ => next_state.push(entry.clone()),
        }
        processed.insert(identity);
    }

    if codex_changed {
        operations.push(Operation::file(
            crate::target::codex::CONFIG_PATH,
            codex.unwrap().bytes(),
        ));
    }
    if claude_changed {
        operations.push(Operation::file(
            crate::target::claude::CONFIG_PATH,
            claude.unwrap().bytes()?,
        ));
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

fn lock_diff_plan(previous: Option<&Lockfile>, next: &Lockfile) -> Vec<String> {
    let mut plan = Vec::new();
    let previous_skills: BTreeMap<_, _> = previous
        .into_iter()
        .flat_map(|lock| &lock.skill_packages)
        .flat_map(|package| {
            package.skills.iter().map(move |skill| {
                (
                    skill.name.as_str(),
                    (package.version.as_str(), skill.sha256.as_str()),
                )
            })
        })
        .collect();
    for package in &next.skill_packages {
        for skill in &package.skills {
            match previous_skills.get(skill.name.as_str()) {
                None => plan.push(format!(
                    "lock skill {} {} {} from {}",
                    skill.name, package.version, skill.sha256, package.source
                )),
                Some((version, digest))
                    if *version != package.version || *digest != skill.sha256 =>
                {
                    plan.push(format!(
                        "lock skill {} {} -> {} {} from {}",
                        skill.name, version, package.version, skill.sha256, package.source
                    ));
                }
                _ => {}
            }
        }
    }
    let next_names: BTreeSet<_> = next
        .skill_packages
        .iter()
        .flat_map(|package| package.skills.iter().map(|skill| skill.name.as_str()))
        .collect();
    for name in previous_skills
        .keys()
        .filter(|name| !next_names.contains(**name))
    {
        plan.push(format!("unlock removed skill {name}"));
    }
    let previous_mcp: BTreeMap<_, _> = previous
        .into_iter()
        .flat_map(|lock| &lock.mcp_servers)
        .map(|server| (server.name.as_str(), server.version.as_str()))
        .collect();
    for server in &next.mcp_servers {
        match previous_mcp.get(server.name.as_str()) {
            None => plan.push(format!("lock MCP {} {}", server.name, server.version)),
            Some(version) if *version != server.version => plan.push(format!(
                "lock MCP {} {} -> {}",
                server.name, version, server.version
            )),
            _ => {}
        }
    }
    plan
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

#[cfg(unix)]
fn supports_project_symlink() -> bool {
    true
}

#[cfg(not(unix))]
fn supports_project_symlink() -> bool {
    false
}

pub fn garbage_collect(project: &Path, lock: &Lockfile) -> Result<()> {
    let referenced = lock
        .skill_packages
        .iter()
        .map(|package| (package.source.clone(), package.revision.clone()))
        .collect::<Vec<_>>();
    Cache::project(project).garbage_collect(&referenced)
}
