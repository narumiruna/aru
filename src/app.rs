use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use clap::Parser;

use crate::cli::{
    Cli, Command, LockArgs, McpAddArgs, McpCommand, McpRemoveArgs, McpUpdateArgs, SkillAddArgs,
    SkillCommand, SkillRemoveArgs, SkillUpdateArgs, SyncArgs,
};
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::Lockfile;
use crate::manifest::{Agent, ManifestDocument, McpRequirement, SkillRequirement, validate_name};
use crate::resolver::canonical_update_skill_targets;
use crate::sync::{SyncOptions, garbage_collect, prepare};
use crate::transaction::{JOURNAL_FILE, Operation, ProjectLock, apply, recover_if_needed};

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init(args) => init(project_for_init(cli.project)?, args.agent),
        Command::Lock(args) => {
            let project = discover_project(cli.project)?;
            lock(&project, args)
        }
        Command::Sync(args) => {
            let project = discover_project(cli.project)?;
            sync(&project, args)
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

fn init(project: PathBuf, mut agents: Vec<Agent>) -> Result<()> {
    if project.join(crate::manifest::MANIFEST_FILE).exists() {
        return Err(AruError::msg("aru.toml already exists"));
    }
    agents.sort();
    agents.dedup();
    if agents.is_empty() {
        return Err(AruError::msg("aru init requires at least one --agent"));
    }
    let _lock = ProjectLock::acquire(&project)?;
    recover_if_needed(&project)?;
    if project.join(crate::manifest::MANIFEST_FILE).exists() {
        return Err(AruError::msg("aru.toml already exists"));
    }
    let manifest = ManifestDocument::new(&agents);
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
        agents
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

fn skill_add(project: &Path, args: SkillAddArgs) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    let key = find_skill_key(project, &manifest, &args.source)?.unwrap_or(args.source.clone());
    let existing = manifest.skills.get(&key);
    let mut requirement = existing.cloned().unwrap_or_default();
    if existing.is_none() && (!args.skills.is_empty() || args.path.is_some()) {
        requirement.include.clear();
    }
    if let Some(version) = args.version {
        requirement.version = Some(version);
        requirement.rev = None;
    }
    if let Some(revision) = args.rev {
        requirement.rev = Some(revision);
        requirement.version = None;
    }
    if args.skills.is_empty() && args.path.is_none() {
        requirement.include = vec!["*".into()];
        requirement.exclude.clear();
    } else {
        for name in args.skills {
            validate_name(&name, "skill name")?;
            add_skill_selector(&mut requirement, &name);
        }
        if let Some(path) = args.path {
            let parsed = crate::skill::validate_relative_selector(&path)?;
            let name = parsed
                .file_name()
                .and_then(|part| part.to_str())
                .ok_or_else(|| AruError::msg("skill path has no UTF-8 directory name"))?
                .to_owned();
            validate_name(&name, "skill name")?;
            add_skill_selector(&mut requirement, &name);
            requirement.paths.insert(name, path);
        }
    }
    requirement.normalize();
    requirement.validate(&key)?;
    document.set_skill(&key, &requirement);
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
        },
    )?;
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
    if prepared.operations.is_empty() {
        println!("aru project is already synchronized");
    } else {
        for item in &prepared.plan {
            println!("{item}");
        }
        apply(project, prepared.operations)?;
    }
    garbage_collect(project, &prepared.lock)?;
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
