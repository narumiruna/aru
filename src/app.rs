mod audit;
mod export;
mod inspection;
mod instruction;
mod mcp;
mod package_archive;
mod package_dependency;
mod skill;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::cli::{
    Cli, Command, InstructionCommand, LockArgs, McpCommand, SkillCommand, SyncArgs, TargetAddArgs,
    TargetCommand, TargetRemoveArgs, TargetSetArgs,
};
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::Lockfile;
use crate::manifest::{ManifestDocument, Target};
use crate::output::Output;
use crate::sync::{
    CollisionPolicy, ReconcileRequest, SyncResult, garbage_collect, prepare_request,
};
use crate::transaction::{JOURNAL_FILE, Operation, ProjectLock, apply, recover_if_needed};

#[derive(Debug, Clone, Copy)]
struct ExecutionPolicy {
    locked: bool,
    offline: bool,
    output: Output,
}

impl Default for ExecutionPolicy {
    fn default() -> Self {
        Self {
            locked: false,
            offline: false,
            output: Output::new(false, 0, crate::cli::ColorChoice::Never, true),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ProjectionPolicy {
    LockOnly,
    Project(CollisionPolicy),
}

impl ExecutionPolicy {
    fn request(self, dry_run: bool, projection: ProjectionPolicy) -> ReconcileRequest {
        match projection {
            ProjectionPolicy::LockOnly => {
                ReconcileRequest::lock_update(self.locked, self.offline, dry_run)
            }
            ProjectionPolicy::Project(collision) => {
                ReconcileRequest::project_update(self.locked, self.offline, dry_run, collision)
            }
        }
    }
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    let project_option = cli.project;
    let policy = ExecutionPolicy {
        locked: cli.locked || cli.frozen,
        offline: cli.offline || cli.frozen,
        output: Output::new(cli.quiet, cli.verbose, cli.color, cli.no_progress),
    };
    match cli.command {
        Command::Init(args) => init_with_output(
            project_for_init(project_option, args.path)?,
            args.target,
            policy.output,
        ),
        Command::Add(args) => {
            let project = discover_project(project_option)?;
            package_dependency::add(&project, args, policy)
        }
        Command::Remove(args) => {
            let project = discover_project(project_option)?;
            package_dependency::remove(&project, args, policy)
        }
        Command::Update(args) => {
            let project = discover_project(project_option)?;
            package_dependency::update(&project, args, policy)
        }
        Command::Lock(args) => {
            let project = discover_project(project_option)?;
            lock(&project, args, policy)
        }
        Command::Sync(args) => {
            let project = discover_project(project_option)?;
            sync(&project, args, policy)
        }
        Command::Audit(args) => {
            let project = discover_project(project_option)?;
            audit::run(&project, args, policy)
        }
        Command::Export(args) => {
            let project = discover_project(project_option)?;
            export::run(&project, args, policy)
        }
        Command::Tree(args) => {
            let project = discover_project(project_option)?;
            inspection::tree(&project, args)
        }
        Command::Info(args) => {
            let project = discover_project(project_option)?;
            inspection::info(&project, args, policy)
        }
        Command::Metadata(args) => {
            let project = discover_project(project_option)?;
            inspection::metadata(&project, args)
        }
        Command::Package(args) => {
            let package = package_for_archive(project_option)?;
            package_archive::run(&package, args, policy)
        }
        Command::Instruction { command } => {
            let project = discover_project(project_option)?;
            match command {
                InstructionCommand::Add(args) => instruction::add(&project, args, policy),
                InstructionCommand::Remove(args) => instruction::remove(&project, args, policy),
                InstructionCommand::List => instruction::list(&project),
            }
        }
        Command::Target { command } => {
            let project = discover_project(project_option)?;
            match command {
                TargetCommand::Add(args) => target_add(&project, args, policy),
                TargetCommand::Remove(args) => target_remove(&project, args, policy),
                TargetCommand::Set(args) => target_set(&project, args, policy),
                TargetCommand::List => target_list(&project),
            }
        }
        Command::Skill { command } => {
            let project = discover_project(project_option)?;
            match command {
                SkillCommand::Add(args) => skill::add(&project, args, policy),
                SkillCommand::Remove(args) => skill::remove(&project, args, policy),
                SkillCommand::Update(args) => skill::update(&project, args, policy),
                SkillCommand::List => skill::list(&project),
            }
        }
        Command::Mcp { command } => {
            let project = discover_project(project_option)?;
            match command {
                McpCommand::Add(args) => mcp::add(&project, *args, policy),
                McpCommand::Remove(args) => mcp::remove(&project, args, policy),
                McpCommand::Update(args) => mcp::update(&project, args, policy),
                McpCommand::List => mcp::list(&project),
            }
        }
    }
}

#[cfg(test)]
fn init(project: PathBuf, targets: Vec<Target>) -> Result<()> {
    init_with_output(project, targets, ExecutionPolicy::default().output)
}

fn init_with_output(project: PathBuf, mut targets: Vec<Target>, output: Output) -> Result<()> {
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
    output.completion(&format!(
        "Initialized aru project for {}.",
        targets
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    ));
    Ok(())
}

fn lock(project: &Path, args: LockArgs, policy: ExecutionPolicy) -> Result<()> {
    let dry_run = args.dry_run || args.check;
    let _guard = begin(project, dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    if args.check {
        return check_execution(project, &manifest, false, policy.output);
    }
    execute(
        project,
        &manifest,
        policy.request(dry_run, ProjectionPolicy::LockOnly),
        policy.output,
    )
}

fn sync(project: &Path, args: SyncArgs, policy: ExecutionPolicy) -> Result<()> {
    let dry_run = args.dry_run || args.check;
    let _guard = begin(project, dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    if args.check {
        return check_execution(project, &manifest, true, policy.output);
    }
    execute(
        project,
        &manifest,
        policy.request(
            dry_run,
            ProjectionPolicy::Project(CollisionPolicy::from_flags(args.merge, args.force)?),
        ),
        policy.output,
    )
}

fn check_execution(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    project_projections: bool,
    output: Output,
) -> Result<()> {
    let previous = Lockfile::load_optional(project)?;
    let request = if project_projections {
        ReconcileRequest::check_project()
    } else {
        ReconcileRequest::check_lock()
    };
    let prepared = prepare_request(project, manifest, previous.as_ref(), request)?;
    if !prepared.operations.is_empty() || (project_projections && !prepared.warnings.is_empty()) {
        let message = if project_projections {
            "project is not synchronized; run `aru sync`"
        } else {
            "aru.lock is not up to date; run `aru lock`"
        };
        return Err(AruError::msg(message));
    }
    if project_projections {
        output.completion("Project is synchronized.");
    } else {
        output.completion("Lockfile is up to date.");
    }
    Ok(())
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

fn target_add(project: &Path, args: TargetAddArgs, policy: ExecutionPolicy) -> Result<()> {
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
        args.merge,
        args.force,
        policy,
    )
}

fn target_remove(project: &Path, args: TargetRemoveArgs, policy: ExecutionPolicy) -> Result<()> {
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
        false,
        policy,
    )
}

fn target_set(project: &Path, args: TargetSetArgs, policy: ExecutionPolicy) -> Result<()> {
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
        args.merge,
        args.force,
        policy,
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
    merge_instructions: bool,
    force: bool,
    policy: ExecutionPolicy,
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
    let projection = if no_sync {
        ProjectionPolicy::LockOnly
    } else {
        ProjectionPolicy::Project(CollisionPolicy::from_flags(merge_instructions, force)?)
    };
    let request = policy
        .request(dry_run, projection)
        .with_manifest_bytes(document.bytes());
    execute_target_change(
        project,
        &manifest,
        request,
        target_plan,
        &targets,
        policy.output,
    )
}

fn normalize_targets(targets: &mut Vec<Target>) {
    targets.sort();
    targets.dedup();
}

fn execute(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    request: ReconcileRequest,
    output: Output,
) -> Result<()> {
    let previous = Lockfile::load_optional(project)?;
    let dry_run = request.dry_run();
    let project_projections = request.projects();
    let deferred = !project_projections && request.changes_intent();
    output.progress("project state");
    let prepared = prepare_request(project, manifest, previous.as_ref(), request)?;
    let changed_completion = if deferred {
        "Target paths were not changed; run `aru sync` to apply."
    } else if project_projections {
        "Project synchronized."
    } else {
        "Lockfile updated."
    };
    let unchanged_completion = if project_projections {
        "Project is synchronized."
    } else {
        "Lockfile is up to date."
    };
    finish_execution(
        project,
        prepared,
        dry_run,
        changed_completion,
        unchanged_completion,
        output,
    )
}

fn execute_target_change(
    project: &Path,
    manifest: &crate::manifest::Manifest,
    request: ReconcileRequest,
    target_plan: Vec<String>,
    targets: &[Target],
    output: Output,
) -> Result<()> {
    let previous = Lockfile::load_optional(project)?;
    let dry_run = request.dry_run();
    let project_projections = request.projects();
    let mut prepared = prepare_request(project, manifest, previous.as_ref(), request)?;
    prepared.plan.extend(target_plan);
    prepared.plan.sort();
    let configured = targets
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    let completion = if project_projections {
        format!("Targets synchronized: {configured}.")
    } else {
        "Target paths were not changed; run `aru sync` to apply.".into()
    };
    finish_execution(
        project,
        prepared,
        dry_run,
        &completion,
        "Project is synchronized.",
        output,
    )
}

fn finish_execution(
    project: &Path,
    prepared: SyncResult,
    dry_run: bool,
    changed_completion: &str,
    unchanged_completion: &str,
    output: Output,
) -> Result<()> {
    for warning in &prepared.warnings {
        output.warning(warning);
    }
    if dry_run {
        for preview in &prepared.previews {
            output.preview(preview);
        }
        for item in &prepared.plan {
            output.plan(item, true);
        }
        for detail in &prepared.details {
            output.detail(detail);
        }
        if output.verbose() > 1 {
            output.detail(&format!(
                "projection input {}",
                prepared.lock.projection_input_hash
            ));
        }
        output.completion("Dry run complete; no files were changed.");
        return Ok(());
    }
    let changed = !prepared.operations.is_empty();
    if changed {
        apply(project, prepared.operations)?;
    }
    garbage_collect(project, &prepared.lock)?;
    if changed {
        for item in &prepared.plan {
            output.plan(item, false);
        }
        for detail in &prepared.details {
            output.detail(detail);
        }
        if output.verbose() > 1 {
            output.detail(&format!(
                "projection input {}",
                prepared.lock.projection_input_hash
            ));
        }
        output.completion(changed_completion);
    } else {
        output.completion(unchanged_completion);
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

fn package_for_archive(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        let path = path.canonicalize().at(&path)?;
        if !path.join(crate::package::PACKAGE_MANIFEST_FILE).is_file() {
            return Err(AruError::msg(format!(
                "no {} in {}",
                crate::package::PACKAGE_MANIFEST_FILE,
                path.display()
            )));
        }
        return Ok(path);
    }
    let current = std::env::current_dir().at(".")?;
    for ancestor in current.ancestors() {
        if ancestor
            .join(crate::package::PACKAGE_MANIFEST_FILE)
            .is_file()
        {
            return ancestor.canonicalize().at(ancestor);
        }
    }
    Err(AruError::msg(format!(
        "no {} found in the current directory or its ancestors",
        crate::package::PACKAGE_MANIFEST_FILE
    )))
}

fn project_for_init(explicit: Option<PathBuf>, positional: Option<PathBuf>) -> Result<PathBuf> {
    if explicit.is_some() && positional.is_some() {
        return Err(AruError::msg(
            "project path was provided both positionally and with --project",
        ));
    }
    let path = positional
        .or(explicit)
        .unwrap_or(std::env::current_dir().at(".")?);
    let path = path.canonicalize().at(&path)?;
    if !path.is_dir() {
        return Err(AruError::msg("project path is not a directory"));
    }
    Ok(path)
}

#[cfg(test)]
use skill::skill_add_with_mode;

#[cfg(test)]
mod tests;
