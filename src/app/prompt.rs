//! Read-only command assistance. Recheck the captured project identity under the
//! operation lock before executing a command based on these choices.
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::cli::{
    Command, InstructionCommand, McpCommand, PluginCommand, SkillCommand, SkillInstallScope,
    SkillTargetArg, TargetCommand,
};
use crate::error::{AruError, IoContext, Result};
use crate::interactive::{self, TargetChoice};
use crate::lockfile::Lockfile;
use crate::manifest::{ManifestDocument, Target};

use super::{AddRoot, discover_add_root, discover_project};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProjectSnapshot {
    manifest: Option<[u8; 32]>,
    lock: Option<[u8; 32]>,
}

impl ProjectSnapshot {
    pub(super) fn read(project: &Path) -> Result<Self> {
        fn digest(path: &Path) -> Result<Option<[u8; 32]>> {
            const MAX_BYTES: u64 = 16 * 1024 * 1024;
            let metadata = match std::fs::metadata(path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error).at(path),
            };
            if !metadata.is_file() || metadata.len() > MAX_BYTES {
                return Err(AruError::msg(format!(
                    "interactive snapshot requires a regular file of at most 16 MiB: {}",
                    path.display()
                )));
            }
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .at(path)?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .at(path)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err(AruError::msg("interactive snapshot exceeds 16 MiB"));
            }
            Ok(Some(Sha256::digest(bytes).into()))
        }
        Ok(Self {
            manifest: digest(&project.join("aru.toml"))?,
            lock: digest(&project.join("aru.lock"))?,
        })
    }

    pub(super) fn verify(self, project: &Path) -> Result<()> {
        if self != Self::read(project)? {
            return Err(AruError::msg(
                "aru.toml or aru.lock changed during interactive selection; retry the command",
            ));
        }
        Ok(())
    }
}

pub(super) enum Prepared {
    Canceled,
    Ready(Option<ProjectSnapshot>),
}

pub(super) fn prepare(
    command: &mut Command,
    project_option: &mut Option<PathBuf>,
    enabled: bool,
) -> Result<Prepared> {
    if let Command::Skill {
        command: SkillCommand::Add(args),
    } = command
    {
        if !args.global && args.scope.is_none() && enabled {
            let Some(scope) = interactive::installation_scope()? else {
                return Ok(Prepared::Canceled);
            };
            args.scope = Some(scope);
        }
        args.global |= args.scope == Some(SkillInstallScope::Global);
    }

    if !enabled {
        validate_required(command)?;
        return Ok(Prepared::Ready(None));
    }
    if let Command::Init(args) = command {
        if args.target.is_empty() {
            // Validate the root and existing intent before opening a menu.
            let root = super::project_for_init(project_option.clone(), args.path.clone())?;
            if root.join("aru.toml").exists() {
                return Err(AruError::msg("aru.toml already exists"));
            }
            if !targets(
                &mut args.target,
                all_targets(),
                &[],
                "Select project targets",
            )? {
                return Ok(Prepared::Canceled);
            }
        }
        return Ok(Prepared::Ready(None));
    }
    if !needs_project_choices(command) {
        return Ok(Prepared::Ready(None));
    }
    let project = match command {
        Command::Skill {
            command: SkillCommand::Add(_),
        }
        | Command::Mcp {
            command: McpCommand::Add(_),
        } => match discover_add_root(project_option.clone())? {
            AddRoot::Managed(project) => project,
            AddRoot::Standalone(_) => return Ok(Prepared::Ready(None)),
        },
        _ => discover_project(project_option.clone())?,
    };
    // Keep subsequent dispatch rooted at the project the menu actually displayed.
    *project_option = Some(project.clone());
    let (snapshot, manifest, previous) = {
        // Never recover or create project state merely to display a menu.
        let _guard = super::begin(&project, true)?;
        (
            ProjectSnapshot::read(&project)?,
            ManifestDocument::load(&project)?.manifest()?,
            Lockfile::load_optional(&project)?,
        )
    };
    let configured = &manifest.project.targets;
    let accepted = match command {
        Command::Add(args) => managed_targets(&mut args.targets, configured.clone())?,
        Command::Remove(args) => one(
            &mut args.source,
            manifest.packages.keys().cloned().collect(),
            "Select package to remove",
        )?,
        Command::Update(args) => {
            let available = previous
                .map(|lock| {
                    lock.aru_packages
                        .into_iter()
                        .map(|package| package.source)
                        .collect()
                })
                .unwrap_or_else(|| manifest.packages.keys().cloned().collect());
            updates(&mut args.packages, available, "Select packages to update")?
        }
        Command::Instruction { command } => match command {
            InstructionCommand::Add(args) => {
                if let Some(path) = interactive::instruction_path()? {
                    args.files = vec![path];
                    true
                } else {
                    false
                }
            }
            InstructionCommand::Remove(args) => {
                let available = manifest
                    .instructions
                    .sources
                    .into_iter()
                    .flat_map(|source| source.files)
                    .collect();
                many(
                    &mut args.files,
                    available,
                    &[],
                    "Select instruction selectors to remove",
                )?
            }
            InstructionCommand::List => unreachable!(),
        },
        Command::Target { command } => match command {
            TargetCommand::Add(args) => targets(
                &mut args.targets,
                all_targets()
                    .into_iter()
                    .filter(|target| !configured.contains(target))
                    .collect(),
                &[],
                "Select targets to add",
            )?,
            TargetCommand::Remove(args) => targets(
                &mut args.targets,
                configured.clone(),
                &[],
                "Select targets to remove",
            )?,
            TargetCommand::Set(args) => targets(
                &mut args.targets,
                all_targets(),
                configured,
                "Select project targets",
            )?,
            TargetCommand::List(_) => unreachable!(),
        },
        Command::Skill { command } => match command {
            SkillCommand::Add(args) => {
                let mut selected = Vec::new();
                let accepted = managed_targets(
                    &mut selected,
                    configured
                        .iter()
                        .copied()
                        .filter(|target| crate::target::capabilities(*target).skills)
                        .collect(),
                )?;
                args.targets = selected
                    .into_iter()
                    .map(SkillTargetArg::canonical)
                    .collect();
                accepted
            }
            SkillCommand::Remove(args) => one(
                &mut args.source,
                manifest.skills.keys().cloned().collect(),
                "Select skill source to remove",
            )?,
            SkillCommand::Update(args) => updates(
                &mut args.sources,
                manifest.skills.keys().cloned().collect(),
                "Select skill sources to update",
            )?,
            SkillCommand::List => unreachable!(),
        },
        Command::Mcp { command } => match command {
            McpCommand::Add(args) => managed_targets(
                &mut args.targets,
                configured
                    .iter()
                    .copied()
                    .filter(|target| crate::target::capabilities(*target).mcp)
                    .collect(),
            )?,
            McpCommand::Remove(args) => one(
                &mut args.name,
                manifest.mcp.keys().cloned().collect(),
                "Select MCP server to remove",
            )?,
            McpCommand::Update(args) => updates(
                &mut args.names,
                manifest
                    .mcp
                    .iter()
                    .filter(|(_, requirement)| requirement.server.is_some())
                    .map(|(name, _)| name.clone())
                    .collect(),
                "Select MCP servers to update",
            )?,
            McpCommand::List => unreachable!(),
        },
        Command::Plugin { command } => match command {
            PluginCommand::Add(args) => managed_targets(&mut args.targets, configured.clone())?,
            PluginCommand::Remove(args) => one(
                &mut args.name,
                manifest.plugins.keys().cloned().collect(),
                "Select plugin to remove",
            )?,
            PluginCommand::Update(args) => updates(
                &mut args.names,
                manifest.plugins.keys().cloned().collect(),
                "Select plugins to update",
            )?,
            _ => unreachable!(),
        },
        _ => unreachable!(),
    };
    Ok(if accepted {
        Prepared::Ready(Some(snapshot))
    } else {
        Prepared::Canceled
    })
}

fn needs_project_choices(command: &Command) -> bool {
    match command {
        Command::Add(args) => args.targets.is_empty(),
        Command::Remove(args) => args.source.is_empty(),
        Command::Update(args) => args.packages.is_empty(),
        Command::Instruction {
            command: InstructionCommand::Add(args),
        } => args.files.is_empty(),
        Command::Instruction {
            command: InstructionCommand::Remove(args),
        } => args.files.is_empty(),
        Command::Target {
            command: TargetCommand::Add(args),
        } => args.targets.is_empty(),
        Command::Target {
            command: TargetCommand::Remove(args),
        } => args.targets.is_empty(),
        Command::Target {
            command: TargetCommand::Set(args),
        } => args.targets.is_empty(),
        Command::Skill {
            command: SkillCommand::Add(args),
        } => !args.global && args.targets.is_empty(),
        Command::Skill {
            command: SkillCommand::Remove(args),
        } => args.source.is_empty(),
        Command::Skill {
            command: SkillCommand::Update(args),
        } => args.sources.is_empty(),
        Command::Mcp {
            command: McpCommand::Add(args),
        } => args.targets.is_empty(),
        Command::Mcp {
            command: McpCommand::Remove(args),
        } => args.name.is_empty(),
        Command::Mcp {
            command: McpCommand::Update(args),
        } => args.names.is_empty(),
        Command::Plugin {
            command: PluginCommand::Add(args),
        } => args.targets.is_empty(),
        Command::Plugin {
            command: PluginCommand::Remove(args),
        } => args.name.is_empty(),
        Command::Plugin {
            command: PluginCommand::Update(args),
        } => args.names.is_empty(),
        _ => false,
    }
}

fn validate_required(command: &Command) -> Result<()> {
    let missing = match command {
        Command::Init(args) if args.target.is_empty() => Some("--target"),
        Command::Remove(args) if args.source.is_empty() => Some("SOURCE"),
        Command::Instruction {
            command: InstructionCommand::Add(args),
        } if args.files.is_empty() => Some("FILE"),
        Command::Instruction {
            command: InstructionCommand::Remove(args),
        } if args.files.is_empty() => Some("FILE"),
        Command::Target {
            command: TargetCommand::Add(args),
        } if args.targets.is_empty() => Some("TARGET"),
        Command::Target {
            command: TargetCommand::Remove(args),
        } if args.targets.is_empty() => Some("TARGET"),
        Command::Target {
            command: TargetCommand::Set(args),
        } if args.targets.is_empty() => Some("TARGET"),
        Command::Skill {
            command: SkillCommand::Remove(args),
        } if args.source.is_empty() => Some("SOURCE"),
        Command::Mcp {
            command: McpCommand::Remove(args),
        } if args.name.is_empty() => Some("NAME"),
        Command::Plugin {
            command: PluginCommand::Remove(args),
        } if args.name.is_empty() => Some("NAME"),
        _ => None,
    };
    if let Some(argument) = missing {
        return Err(AruError::msg(format!(
            "interactive selection requires a terminal and prompts enabled; pass {argument}"
        )));
    }
    Ok(())
}

fn all_targets() -> Vec<Target> {
    crate::target::specs()
        .iter()
        .map(|spec| spec.target)
        .collect()
}

fn targets(
    selected: &mut Vec<Target>,
    available: Vec<Target>,
    defaults: &[Target],
    message: &str,
) -> Result<bool> {
    let choices = available
        .into_iter()
        .map(|target| TargetChoice::new(target, crate::target::spec(target).project_skills))
        .collect::<Vec<_>>();
    let defaults = choices
        .iter()
        .filter(|choice| defaults.contains(&choice.target))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let Some(values) = interactive::select_many(message, choices, &defaults)? else {
        return Ok(false);
    };
    *selected = values.into_iter().map(|choice| choice.target).collect();
    selected.sort();
    Ok(true)
}

fn managed_targets(selected: &mut Vec<Target>, available: Vec<Target>) -> Result<bool> {
    targets(
        selected,
        available.clone(),
        &available,
        "Select dependency targets",
    )
}

fn one(selected: &mut String, available: Vec<String>, message: &str) -> Result<bool> {
    let Some(value) = interactive::select_one(message, available)? else {
        return Ok(false);
    };
    *selected = value;
    Ok(true)
}

fn many(
    selected: &mut Vec<String>,
    mut available: Vec<String>,
    defaults: &[String],
    message: &str,
) -> Result<bool> {
    available.sort();
    available.dedup();
    let Some(values) = interactive::select_many(message, available, defaults)? else {
        return Ok(false);
    };
    *selected = values;
    selected.sort();
    Ok(true)
}

fn updates(selected: &mut Vec<String>, available: Vec<String>, message: &str) -> Result<bool> {
    many(selected, available.clone(), &available, message)
}

#[cfg(test)]
mod tests;
