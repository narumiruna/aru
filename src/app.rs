use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::cli::{
    Cli, Command, LockArgs, McpAddArgs, McpCommand, McpRemoveArgs, McpUpdateArgs, SkillAddArgs,
    SkillCommand, SkillRemoveArgs, SkillUpdateArgs, SyncArgs, TargetAddArgs, TargetCommand,
    TargetRemoveArgs, TargetSetArgs,
};
use crate::error::{AruError, IoContext, Result};
use crate::interactive::{
    InquireSkillChooser, SkillAddSelectionMode, SkillChooser, choose_skills,
    terminal_selection_mode,
};
use crate::lockfile::Lockfile;
use crate::manifest::{ManifestDocument, McpRequirement, SkillRequirement, Target, validate_name};
use crate::resolver::{SkillResolutionHint, canonical_update_skill_targets, inspect_skill_source};
use crate::sync::{SyncOptions, SyncResult, garbage_collect, prepare};
use crate::transaction::{JOURNAL_FILE, Operation, ProjectLock, apply, recover_if_needed};

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => init(project_for_init(cli.project)?, args.target),
        Command::Lock(args) => {
            let project = discover_project(cli.project)?;
            lock(&project, args)
        }
        Command::Sync(args) => {
            let project = discover_project(cli.project)?;
            sync(&project, args)
        }
        Command::Target { command } => {
            let project = discover_project(cli.project)?;
            match command {
                TargetCommand::Add(args) => target_add(&project, args),
                TargetCommand::Remove(args) => target_remove(&project, args),
                TargetCommand::Set(args) => target_set(&project, args),
                TargetCommand::List => target_list(&project),
            }
        }
        Command::Skill { command } => {
            let project = discover_project(cli.project)?;
            match command {
                SkillCommand::Add(args) => skill_add(&project, args),
                SkillCommand::Remove(args) => skill_remove(&project, args),
                SkillCommand::Update(args) => skill_update(&project, args),
            }
        }
        Command::Mcp { command } => {
            let project = discover_project(cli.project)?;
            match command {
                McpCommand::Add(args) => mcp_add(&project, args),
                McpCommand::Remove(args) => mcp_remove(&project, args),
                McpCommand::Update(args) => mcp_update(&project, args),
            }
        }
    }
}

fn init(project: PathBuf, mut targets: Vec<Target>) -> Result<()> {
    if project.join(crate::manifest::MANIFEST_FILE).exists() {
        return Err(AruError::msg("aru.toml already exists"));
    }
    targets.sort();
    targets.dedup();
    if targets.is_empty() {
        return Err(AruError::msg("aru init requires at least one --target"));
    }
    let _lock = ProjectLock::acquire(&project)?;
    recover_if_needed(&project)?;
    if project.join(crate::manifest::MANIFEST_FILE).exists() {
        return Err(AruError::msg("aru.toml already exists"));
    }
    let manifest = ManifestDocument::new(&targets);
    manifest.manifest()?;
    let mut operations = vec![Operation::file(
        crate::manifest::MANIFEST_FILE,
        manifest.bytes(),
    )];
    let gitignore_path = project.join(".gitignore");
    let existing = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path).at(&gitignore_path)?
    } else {
        String::new()
    };
    if !existing.lines().any(|line| line.trim() == ".aru/") {
        let mut updated = existing;
        if !updated.is_empty() && !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push_str(".aru/\n");
        operations.push(Operation::file(".gitignore", updated.into_bytes()));
    }
    apply(&project, operations)?;
    println!(
        "initialized aru project for {}",
        targets
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

fn lock(project: &Path, args: LockArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    execute(
        project,
        &manifest,
        None,
        args.dry_run,
        false,
        false,
        false,
        BTreeSet::new(),
        BTreeSet::new(),
    )
}

fn sync(project: &Path, args: SyncArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    execute(
        project,
        &manifest,
        None,
        args.dry_run,
        args.locked,
        true,
        args.force,
        BTreeSet::new(),
        BTreeSet::new(),
    )
}

fn target_list(project: &Path) -> Result<()> {
    let manifest = ManifestDocument::load(project)?.manifest()?;
    let mut targets = manifest.project.targets;
    targets.sort();
    for target in targets {
        println!("{target}");
    }
    Ok(())
}

fn target_add(project: &Path, args: TargetAddArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let current = document.manifest()?.project.targets;
    let mut targets = current.clone();
    targets.extend(args.targets);
    normalize_targets(&mut targets);
    apply_target_change(
        project,
        document,
        current,
        targets,
        args.no_sync,
        args.dry_run,
        args.force,
    )
}

fn target_remove(project: &Path, args: TargetRemoveArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let current = document.manifest()?.project.targets;
    let mut requested = args.targets;
    normalize_targets(&mut requested);
    for target in &requested {
        if !current.contains(target) {
            return Err(AruError::msg(format!(
                "target \"{target}\" is not configured"
            )));
        }
    }
    let targets = current
        .iter()
        .copied()
        .filter(|target| !requested.contains(target))
        .collect::<Vec<_>>();
    if targets.is_empty() {
        return Err(AruError::msg(
            "cannot remove the last target; use `aru target set <TARGET>` to switch targets",
        ));
    }
    apply_target_change(
        project,
        document,
        current,
        targets,
        args.no_sync,
        args.dry_run,
        false,
    )
}

fn target_set(project: &Path, args: TargetSetArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let current = document.manifest()?.project.targets;
    let mut targets = args.targets;
    normalize_targets(&mut targets);
    if targets.is_empty() {
        return Err(AruError::msg("aru target set requires at least one target"));
    }
    apply_target_change(
        project,
        document,
        current,
        targets,
        args.no_sync,
        args.dry_run,
        args.force,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_target_change(
    project: &Path,
    mut document: ManifestDocument,
    current: Vec<Target>,
    targets: Vec<Target>,
    no_sync: bool,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    document.set_targets(&targets);
    let manifest = document.manifest()?;
    let current_set: BTreeSet<_> = current.into_iter().collect();
    let target_set: BTreeSet<_> = targets.iter().copied().collect();
    let mut target_plan = target_set
        .difference(&current_set)
        .map(|target| format!("add target {target}"))
        .chain(
            current_set
                .difference(&target_set)
                .map(|target| format!("remove target {target}")),
        )
        .collect::<Vec<_>>();
    target_plan.sort();
    execute_target_change(
        project,
        &manifest,
        document.bytes(),
        dry_run,
        !no_sync,
        force,
        target_plan,
        &targets,
    )
}

fn normalize_targets(targets: &mut Vec<Target>) {
    targets.sort();
    targets.dedup();
}

fn skill_add(project: &Path, args: SkillAddArgs) -> Result<()> {
    let mode = terminal_selection_mode(args.all, !args.skills.is_empty(), args.path.is_some())?;
    let mut chooser = InquireSkillChooser;
    skill_add_with_mode(project, args, mode, &mut chooser)
}

fn skill_add_with_mode(
    project: &Path,
    args: SkillAddArgs,
    mode: SkillAddSelectionMode,
    chooser: &mut dyn SkillChooser,
) -> Result<()> {
    if mode != SkillAddSelectionMode::Interactive {
        return skill_add_explicit(project, &args, mode);
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
        let previous = Lockfile::load_optional(project)?;
        (snapshot, key, requirement, existing, previous)
    };

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
        println!("skill selection canceled");
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
        false,
        !args.no_sync,
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
        false,
        !args.no_sync,
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

fn skill_remove(project: &Path, args: SkillRemoveArgs) -> Result<()> {
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
        false,
        !args.no_sync,
        args.force,
        BTreeSet::new(),
        BTreeSet::new(),
    )
}

fn skill_update(project: &Path, args: SkillUpdateArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let updates = canonical_update_skill_targets(project, &manifest, &args.sources)?;
    execute(
        project,
        &manifest,
        None,
        args.dry_run,
        false,
        !args.no_sync,
        args.force,
        updates,
        BTreeSet::new(),
    )
}

fn mcp_add(project: &Path, args: McpAddArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    validate_name(&args.name, "MCP name")?;
    if let Some(transport) = &args.transport
        && !matches!(transport.as_str(), "stdio" | "streamable-http")
    {
        return Err(AruError::msg(
            "MCP transport must be stdio or streamable-http",
        ));
    }
    if args.url.is_some()
        && args
            .transport
            .as_deref()
            .is_some_and(|value| value != "streamable-http")
    {
        return Err(AruError::msg(
            "direct MCP URLs require streamable-http transport",
        ));
    }
    let mut document = ManifestDocument::load(project)?;
    let requirement = McpRequirement {
        registry: args.server.as_ref().map(|_| {
            args.registry
                .unwrap_or_else(|| crate::registry::DEFAULT_REGISTRY.into())
        }),
        server: args.server,
        version: args.version,
        transport: args.transport,
        package_registry: args.package_registry,
        url: args.url,
        bearer_token_env: args.bearer_token_env,
    };
    requirement.validate(&args.name)?;
    document.set_mcp(&args.name, &requirement);
    let manifest = document.manifest()?;
    execute(
        project,
        &manifest,
        Some(document.bytes()),
        args.dry_run,
        false,
        !args.no_sync,
        args.force,
        BTreeSet::new(),
        BTreeSet::new(),
    )
}

fn mcp_remove(project: &Path, args: McpRemoveArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    if !manifest.mcp.contains_key(&args.name) {
        return Err(AruError::msg(format!(
            "MCP {:?} is not declared",
            args.name
        )));
    }
    document.remove_mcp(&args.name);
    let manifest = document.manifest()?;
    execute(
        project,
        &manifest,
        Some(document.bytes()),
        args.dry_run,
        false,
        !args.no_sync,
        args.force,
        BTreeSet::new(),
        BTreeSet::new(),
    )
}

fn mcp_update(project: &Path, args: McpUpdateArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let updates: BTreeSet<String> = if args.names.is_empty() {
        manifest.mcp.keys().cloned().collect()
    } else {
        for name in &args.names {
            if !manifest.mcp.contains_key(name) {
                return Err(AruError::msg(format!("MCP {name:?} is not declared")));
            }
        }
        args.names.into_iter().collect()
    };
    execute(
        project,
        &manifest,
        None,
        args.dry_run,
        false,
        !args.no_sync,
        args.force,
        BTreeSet::new(),
        updates,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    manifest_bytes: Option<Vec<u8>>,
    dry_run: bool,
    locked: bool,
    project_projections: bool,
    force: bool,
    update_skills: BTreeSet<String>,
    update_mcp: BTreeSet<String>,
) -> Result<()> {
    execute_with_skill_hints(
        project,
        manifest,
        manifest_bytes,
        dry_run,
        locked,
        project_projections,
        force,
        update_skills,
        update_mcp,
        &BTreeMap::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_with_skill_hints(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    manifest_bytes: Option<Vec<u8>>,
    dry_run: bool,
    locked: bool,
    project_projections: bool,
    force: bool,
    update_skills: BTreeSet<String>,
    update_mcp: BTreeSet<String>,
    skill_hints: &BTreeMap<String, SkillResolutionHint>,
) -> Result<()> {
    let previous = Lockfile::load_optional(project)?;
    let prepared = prepare(
        project,
        manifest,
        SyncOptions {
            previous: previous.as_ref(),
            locked,
            dry_run,
            project_projections,
            force,
            manifest_bytes,
            update_skills: &update_skills,
            update_mcp: &update_mcp,
            skill_hints,
        },
    )?;
    finish_execution(project, prepared, dry_run, None)
}

#[allow(clippy::too_many_arguments)]
fn execute_target_change(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    manifest_bytes: Vec<u8>,
    dry_run: bool,
    project_projections: bool,
    force: bool,
    target_plan: Vec<String>,
    targets: &[Target],
) -> Result<()> {
    let previous = Lockfile::load_optional(project)?;
    let update_skills = BTreeSet::new();
    let update_mcp = BTreeSet::new();
    let skill_hints = BTreeMap::new();
    let mut prepared = prepare(
        project,
        manifest,
        SyncOptions {
            previous: previous.as_ref(),
            locked: false,
            dry_run,
            project_projections,
            force,
            manifest_bytes: Some(manifest_bytes),
            update_skills: &update_skills,
            update_mcp: &update_mcp,
            skill_hints: &skill_hints,
        },
    )?;
    prepared.plan.extend(target_plan);
    prepared.plan.sort();
    let configured = targets
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let completion = if project_projections {
        format!("targets synchronized: {configured}")
    } else {
        "target paths were not changed (--no-sync); run `aru sync` to apply".into()
    };
    finish_execution(project, prepared, dry_run, Some(completion))
}

fn finish_execution(
    project: &Path,
    prepared: SyncResult,
    dry_run: bool,
    completion: Option<String>,
) -> Result<()> {
    for warning in &prepared.warnings {
        eprintln!("warning: {warning}");
    }
    if dry_run {
        if prepared.plan.is_empty() {
            println!("dry-run: no changes");
        } else {
            for item in &prepared.plan {
                println!("dry-run: {item}");
            }
        }
        return Ok(());
    }
    let changed = !prepared.operations.is_empty();
    if !changed {
        println!("aru project is already synchronized");
    } else {
        for item in &prepared.plan {
            println!("{item}");
        }
        apply(project, prepared.operations)?;
    }
    garbage_collect(project, &prepared.lock)?;
    if changed && let Some(completion) = completion {
        println!("{completion}");
    }
    Ok(())
}

fn begin(project: &Path, dry_run: bool) -> Result<Option<ProjectLock>> {
    if dry_run {
        if project.join(JOURNAL_FILE).exists() {
            return Err(AruError::msg(
                "a recoverable transaction is pending; run a mutating aru command before --dry-run",
            ));
        }
        Ok(None)
    } else {
        let guard = ProjectLock::acquire(project)?;
        if recover_if_needed(project)? {
            eprintln!("recovered an interrupted aru transaction");
        }
        Ok(Some(guard))
    }
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

fn discover_project(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        let path = path.canonicalize().at(&path)?;
        if !path.join(crate::manifest::MANIFEST_FILE).is_file() {
            return Err(AruError::msg(format!("no aru.toml in {}", path.display())));
        }
        return Ok(path);
    }
    let current = std::env::current_dir().at(".")?;
    for ancestor in current.ancestors() {
        if ancestor.join(crate::manifest::MANIFEST_FILE).is_file() {
            return ancestor.canonicalize().at(ancestor);
        }
    }
    Err(AruError::msg(
        "no aru.toml found in the current directory or its ancestors; run aru init",
    ))
}

fn project_for_init(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let path = explicit.unwrap_or(std::env::current_dir().at(".")?);
    let path = path.canonicalize().at(&path)?;
    if !path.is_dir() {
        return Err(AruError::msg("project path is not a directory"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests;
