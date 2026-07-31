use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::digest::sha256_bytes;
use crate::error::{AruError, IoContext, Result};
use crate::instruction::document::{ManagedDocument, semantic_digest};
use crate::instruction::{DiscoveredInstruction, InstructionUnit};
use crate::lockfile::{LockedInstructionSource, Lockfile};
use crate::ownership::{OwnershipAction, State, StateEntry, reconcile};
use crate::target::instructions::{self, InstructionProjection, ProjectionMode};
use crate::transaction::Operation;

#[allow(clippy::too_many_arguments)]
pub fn prepare(
    project: &Path,
    instructions: &[DiscoveredInstruction],
    lock: &Lockfile,
    previous: Option<&Lockfile>,
    state: &State,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
    merge: bool,
    force: bool,
    lock_identity: &str,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    warnings: &mut Vec<String>,
    next_state: &mut Vec<StateEntry>,
    processed: &mut BTreeSet<(String, String, String)>,
) -> Result<()> {
    let projections = instructions::render(instructions)?;
    let mut shared = BTreeMap::<PathBuf, Vec<InstructionProjection>>::new();
    let mut files = Vec::new();
    for projection in projections {
        match projection.mode {
            ProjectionMode::SharedBlock => shared
                .entry(projection.destination.clone())
                .or_default()
                .push(projection),
            ProjectionMode::File => files.push(projection),
        }
    }

    for projection in files {
        prepare_file(
            project,
            &projection,
            lock,
            state_map,
            force,
            lock_identity,
            operations,
            plan,
            next_state,
            processed,
        )?;
    }

    let stale_shared_destinations = state
        .entries
        .iter()
        .filter(|entry| entry.kind == "instruction" && entry.mode == "merge")
        .map(|entry| PathBuf::from(&entry.destination))
        .collect::<BTreeSet<_>>();
    for destination in stale_shared_destinations {
        shared.entry(destination).or_default();
    }
    for (destination, desired) in shared {
        prepare_shared_document(
            project,
            &destination,
            &desired,
            lock,
            state,
            state_map,
            merge,
            force,
            lock_identity,
            operations,
            plan,
            next_state,
            processed,
        )?;
    }

    for entry in &state.entries {
        let identity = identity(entry);
        if entry.kind != "instruction" || processed.contains(&identity) {
            continue;
        }
        if entry.mode != "file" {
            next_state.push(entry.clone());
            processed.insert(identity);
            continue;
        }
        let destination = PathBuf::from(&entry.destination);
        let current = observe_file(project, &destination)?;
        match reconcile(
            &entry.key,
            current.as_deref(),
            Some(entry),
            None,
            None,
            force,
        )? {
            OwnershipAction::Remove => {
                operations.push(Operation::remove(destination));
                plan.push(format!(
                    "remove instruction {} ({})",
                    entry.key, entry.destination
                ));
            }
            OwnershipAction::ForgetMissing => {
                plan.push(format!("forget missing instruction {}", entry.key));
            }
            _ => next_state.push(entry.clone()),
        }
        processed.insert(identity);
    }

    warn_unowned_removed(project, previous, lock, state_map, warnings)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prepare_file(
    project: &Path,
    projection: &InstructionProjection,
    lock: &Lockfile,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
    force: bool,
    lock_identity: &str,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    next_state: &mut Vec<StateEntry>,
    processed: &mut BTreeSet<(String, String, String)>,
) -> Result<()> {
    let destination = portable(&projection.destination)?;
    let identity = (
        "instruction".into(),
        projection.source.clone(),
        destination.clone(),
    );
    let owned = state_map.get(&identity).copied();
    let current = observe_file(project, &projection.destination)?;
    let desired = sha256_bytes(projection.content.as_bytes());
    let baseline = baseline(lock, projection);
    let action = reconcile(
        &projection.source,
        current.as_deref(),
        owned,
        baseline,
        Some(&desired),
        force,
    )?;
    match action {
        OwnershipAction::Create | OwnershipAction::Update => {
            operations.push(Operation::file(
                projection.destination.clone(),
                projection.content.as_bytes().to_vec(),
            ));
            let verb = if action == OwnershipAction::Create {
                "create"
            } else if force && owned.is_none() {
                "force replace"
            } else {
                "update"
            };
            plan.push(format!(
                "{verb} instruction {} ({destination})",
                projection.source
            ));
        }
        OwnershipAction::Adopt => plan.push(format!(
            "adopt instruction {} ({destination})",
            projection.source
        )),
        OwnershipAction::Noop => {}
        _ => return Err(AruError::msg("unexpected instruction ownership action")),
    }
    next_state.push(StateEntry {
        destination,
        kind: "instruction".into(),
        key: projection.source.clone(),
        mode: "file".into(),
        last_applied_digest: desired,
        lock_identity: lock_identity.into(),
    });
    processed.insert(identity);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prepare_shared_document(
    project: &Path,
    destination: &Path,
    desired: &[InstructionProjection],
    lock: &Lockfile,
    state: &State,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
    merge: bool,
    force: bool,
    lock_identity: &str,
    operations: &mut Vec<Operation>,
    plan: &mut Vec<String>,
    next_state: &mut Vec<StateEntry>,
    processed: &mut BTreeSet<(String, String, String)>,
) -> Result<()> {
    let destination_text = portable(destination)?;
    let path = project.join(destination);
    let existing = if path.exists() {
        Some(std::fs::read_to_string(&path).at(&path)?)
    } else {
        None
    };
    let mut document = match existing.as_deref() {
        Some(text) => match ManagedDocument::parse(text) {
            Ok(document) => document,
            Err(_)
                if force
                    && !state.entries.iter().any(|entry| {
                        entry.kind == "instruction" && entry.destination == destination_text
                    }) =>
            {
                ManagedDocument::empty()
            }
            Err(error) => return Err(error),
        },
        None => ManagedDocument::empty(),
    };
    let destination_owned = state
        .entries
        .iter()
        .any(|entry| entry.kind == "instruction" && entry.destination == destination_text);
    if existing.is_some()
        && !destination_owned
        && !document.has_blocks()
        && document.has_unmanaged_content()
    {
        if force {
            document.remove_all_content();
            plan.push(format!(
                "force replace instruction document ({destination_text})"
            ));
        } else if !merge {
            return Err(AruError::msg(format!(
                "collision: {destination_text} already contains unmanaged content; rerun with --merge to preserve it or --force to replace it"
            )));
        }
    }
    let before = document.render();
    let desired_sources = desired
        .iter()
        .map(|projection| projection.source.as_str())
        .collect::<BTreeSet<_>>();

    for projection in desired {
        let identity = (
            "instruction".into(),
            projection.source.clone(),
            destination_text.clone(),
        );
        let owned = state_map.get(&identity).copied();
        let current = document.block_digest(&projection.source);
        let desired_digest = semantic_digest(&projection.content);
        let action = reconcile(
            &projection.source,
            current.as_deref(),
            owned,
            baseline(lock, projection),
            Some(&desired_digest),
            force,
        )?;
        match action {
            OwnershipAction::Create | OwnershipAction::Update => {
                document.set_block(&projection.source, &projection.content);
                let verb = if action == OwnershipAction::Create {
                    "create"
                } else {
                    "update"
                };
                plan.push(format!(
                    "{verb} instruction {} ({destination_text})",
                    projection.source
                ));
            }
            OwnershipAction::Adopt => plan.push(format!(
                "adopt instruction {} ({destination_text})",
                projection.source
            )),
            OwnershipAction::Noop => {}
            _ => return Err(AruError::msg("unexpected instruction ownership action")),
        }
        next_state.push(StateEntry {
            destination: destination_text.clone(),
            kind: "instruction".into(),
            key: projection.source.clone(),
            mode: "merge".into(),
            last_applied_digest: desired_digest,
            lock_identity: lock_identity.into(),
        });
        processed.insert(identity);
    }

    for entry in state.entries.iter().filter(|entry| {
        entry.kind == "instruction"
            && entry.mode == "merge"
            && entry.destination == destination_text
            && !desired_sources.contains(entry.key.as_str())
    }) {
        let identity = identity(entry);
        let current = document.block_digest(&entry.key);
        match reconcile(
            &entry.key,
            current.as_deref(),
            Some(entry),
            None,
            None,
            force,
        )? {
            OwnershipAction::Remove => {
                document.remove_block(&entry.key);
                plan.push(format!(
                    "remove instruction {} ({destination_text})",
                    entry.key
                ));
            }
            OwnershipAction::ForgetMissing => {
                plan.push(format!("forget missing instruction {}", entry.key));
            }
            _ => next_state.push(entry.clone()),
        }
        processed.insert(identity);
    }

    let after = document.render();
    if after != before {
        if document.is_effectively_empty() {
            if existing.is_some() {
                operations.push(Operation::remove(destination.to_path_buf()));
            }
        } else {
            operations.push(Operation::file(
                destination.to_path_buf(),
                after.into_bytes(),
            ));
        }
    }
    Ok(())
}

pub fn warn_unowned_removed(
    project: &Path,
    previous: Option<&Lockfile>,
    current: &Lockfile,
    state_map: &BTreeMap<(String, String, String), &StateEntry>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    let current_baselines = current
        .projection_baselines
        .iter()
        .filter(|baseline| baseline.kind == "instruction")
        .map(|baseline| (baseline.target, baseline.key.as_str()))
        .collect::<BTreeSet<_>>();
    for projection in projections_from_locked(&previous.instruction_sources)? {
        if current_baselines.contains(&(projection.target, projection.source.as_str())) {
            continue;
        }
        let destination = portable(&projection.destination)?;
        let identity = (
            "instruction".into(),
            projection.source.clone(),
            destination.clone(),
        );
        if !state_map.contains_key(&identity)
            && std::fs::symlink_metadata(project.join(&projection.destination)).is_ok()
        {
            warnings.push(format!(
                "no local ownership record for removed {} instruction {:?}; preserved {destination} for manual review",
                projection.target, projection.source
            ));
        }
    }
    Ok(())
}

fn projections_from_locked(
    sources: &[LockedInstructionSource],
) -> Result<Vec<InstructionProjection>> {
    let units = sources
        .iter()
        .map(|source| DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from(&source.source),
                scope: source.scope.clone(),
                targets: source.targets.iter().copied().collect(),
                source_sha256: source.sha256.clone(),
                managed: source.managed,
            },
            content: String::new(),
        })
        .collect::<Vec<_>>();
    instructions::render(&units)
}

fn baseline<'a>(lock: &'a Lockfile, projection: &InstructionProjection) -> Option<&'a str> {
    lock.projection_baselines
        .iter()
        .find(|baseline| {
            baseline.target == projection.target
                && baseline.kind == "instruction"
                && baseline.key == projection.source
        })
        .map(|baseline| baseline.sha256.as_str())
}

fn observe_file(project: &Path, relative: &Path) -> Result<Option<String>> {
    let path = project.join(relative);
    if !path.exists() {
        return Ok(None);
    }
    let metadata = std::fs::symlink_metadata(&path).at(&path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Ok(Some(format!(
            "wrong-type:{}",
            crate::transaction::path_digest(&path)?.unwrap_or_default()
        )));
    }
    Ok(Some(sha256_bytes(&std::fs::read(&path).at(&path)?)))
}

fn identity(entry: &StateEntry) -> (String, String, String) {
    (
        entry.kind.clone(),
        entry.key.clone(),
        entry.destination.clone(),
    )
}

fn portable(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| AruError::msg("instruction projection path is not UTF-8"))
}
