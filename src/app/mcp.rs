use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cli::{McpAddArgs, McpRemoveArgs, McpUpdateArgs};
use crate::error::{AruError, Result};
use crate::interactive::{InquireTargetChooser, TargetChoice, terminal_choose_targets};
use crate::lockfile::{Lockfile, McpServer};
use crate::manifest::{ManifestDocument, McpRequirement, Target, validate_name};
use crate::sync::{CollisionPolicy, UpdateSelection};
use crate::target::mcp::McpConfig;
use crate::transaction::{Operation, apply_standalone_prepared, validate_standalone_dry_run};

use super::{ExecutionPolicy, ProjectionPolicy};

pub(super) fn add(project: &Path, args: McpAddArgs, policy: ExecutionPolicy) -> Result<()> {
    let _guard = super::begin(project, args.dry_run)?;
    let intent = mcp_add_intent(args)?;
    let mut document = ManifestDocument::load(project)?;
    document.set_mcp(&intent.name, &intent.requirement);
    let manifest = document.manifest()?;
    let projection = if intent.no_sync {
        ProjectionPolicy::LockOnly
    } else {
        ProjectionPolicy::Project(if intent.force {
            CollisionPolicy::Force
        } else {
            CollisionPolicy::Reject
        })
    };
    let request = policy
        .request(intent.dry_run, projection)
        .with_manifest_bytes(document.bytes());
    super::execute(project, &manifest, request, policy.output)
}

pub(super) fn add_standalone(
    project: &Path,
    mut args: McpAddArgs,
    policy: ExecutionPolicy,
) -> Result<()> {
    if args.no_sync {
        return Err(AruError::msg(
            "--no-sync requires an initialized aru project; standalone MCP installation writes directly to target config files",
        ));
    }
    if policy.locked {
        return Err(AruError::msg(
            "--locked and --frozen require an initialized aru project with aru.lock",
        ));
    }
    if args.targets.is_empty() {
        let choices = crate::target::specs()
            .iter()
            .filter(|spec| spec.capabilities.mcp)
            .map(|spec| {
                TargetChoice::new(
                    spec.target,
                    crate::target::mcp::destination(spec.target)
                        .expect("MCP-capable targets have config destinations"),
                )
            })
            .collect::<Vec<_>>();
        let mut chooser = InquireTargetChooser;
        let Some(targets) = terminal_choose_targets(&mut chooser, &choices)? else {
            policy
                .output
                .completion("Target selection canceled; no files were changed.");
            return Ok(());
        };
        args.targets = targets;
    }
    let intent = mcp_add_intent(args)?;
    validate_standalone_targets(&intent.name, &intent.requirement, &intent.targets)?;
    policy.output.progress(&format!("MCP {}", intent.name));
    let server = crate::resolver::resolve_mcp_requirement(
        &intent.name,
        &intent.requirement,
        &intent.targets,
        policy.offline,
    )?;
    let plan = if intent.dry_run {
        let (operations, plan) =
            prepare_standalone_mcp(project, &intent.name, &server, intent.force)?;
        validate_standalone_dry_run(project, &operations)?;
        for item in &plan {
            policy.output.plan(item, true);
        }
        policy
            .output
            .completion("Dry run complete; no files were changed.");
        return Ok(());
    } else {
        apply_standalone_prepared(project, || {
            prepare_standalone_mcp(project, &intent.name, &server, intent.force)
        })?
    };
    for item in &plan {
        policy.output.plan(item, false);
    }
    policy
        .output
        .completion("Standalone MCP installed; no aru project state was created.");
    Ok(())
}

struct McpAddIntent {
    name: String,
    requirement: McpRequirement,
    targets: Vec<Target>,
    no_sync: bool,
    dry_run: bool,
    force: bool,
}

fn mcp_add_intent(args: McpAddArgs) -> Result<McpAddIntent> {
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
    if args.command.is_some() && args.registry.is_some() {
        return Err(AruError::msg(
            "direct stdio MCP cannot set registry or package-registry",
        ));
    }
    let env_http_headers = parse_header_env(&args.header_env)?;
    let mut targets = args.targets;
    targets.sort();
    let mut requirement = McpRequirement {
        registry: args.server.as_ref().map(|_| {
            args.registry
                .unwrap_or_else(|| crate::registry::DEFAULT_REGISTRY.into())
        }),
        server: args.server,
        version: args.version,
        transport: args.transport,
        package_registry: args.package_registry,
        url: args.url,
        command: args.command,
        args: args.args,
        env_vars: args.env_vars,
        env_http_headers,
        bearer_token_env: args.bearer_token_env,
        targets: (!targets.is_empty()).then_some(targets.clone()),
    };
    requirement.validate(&args.name)?;
    requirement.normalize();
    Ok(McpAddIntent {
        name: args.name,
        requirement,
        targets,
        no_sync: args.no_sync,
        dry_run: args.dry_run,
        force: args.force,
    })
}

fn validate_standalone_targets(
    name: &str,
    requirement: &McpRequirement,
    targets: &[Target],
) -> Result<()> {
    if targets.is_empty() {
        return Err(AruError::msg(
            "standalone MCP installation requires a target",
        ));
    }
    requirement.validate_targets(name, targets)
}

fn prepare_standalone_mcp(
    project: &Path,
    name: &str,
    server: &McpServer,
    force: bool,
) -> Result<(Vec<Operation>, Vec<String>)> {
    let mut configs = server
        .targets
        .iter()
        .map(|target| McpConfig::load(project, target.target).map(|config| (target.target, config)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    let mut existing = BTreeSet::new();
    for target in &server.targets {
        let config = configs
            .get(&target.target)
            .expect("resolved MCP targets have loaded config adapters");
        if config.digest(name)?.is_some() {
            if !force {
                let destination = crate::target::mcp::destination(target.target)
                    .expect("resolved MCP targets have config destinations");
                return Err(AruError::msg(format!(
                    "collision: unmanaged MCP {name:?} already exists in {destination}; inspect it or rerun with --force"
                )));
            }
            existing.insert(target.target);
        }
    }
    let mut operations = Vec::new();
    let mut plan = Vec::new();
    for target in &server.targets {
        let config = configs
            .get_mut(&target.target)
            .expect("resolved MCP targets have loaded config adapters");
        config.set(name, target)?;
        let destination = crate::target::mcp::destination(target.target)
            .expect("resolved MCP targets have config destinations");
        let verb = if existing.contains(&target.target) {
            "force replace"
        } else {
            "create"
        };
        plan.push(format!("{verb} MCP {name} ({destination})"));
        operations.push(Operation::file(destination, config.bytes()?));
    }
    plan.sort();
    Ok((operations, plan))
}

fn parse_header_env(assignments: &[String]) -> Result<BTreeMap<String, String>> {
    let mut headers = BTreeMap::new();
    let mut normalized = BTreeSet::new();
    for assignment in assignments {
        let (header, env) = assignment.split_once('=').ok_or_else(|| {
            AruError::msg(format!(
                "invalid --header-env {assignment:?}; expected HEADER=ENV"
            ))
        })?;
        if header.is_empty() || env.is_empty() {
            return Err(AruError::msg(format!(
                "invalid --header-env {assignment:?}; expected non-empty HEADER=ENV"
            )));
        }
        if !normalized.insert(header.to_ascii_lowercase()) {
            return Err(AruError::msg(format!(
                "duplicate --header-env for HTTP header {header:?}"
            )));
        }
        headers.insert(header.to_owned(), env.to_owned());
    }
    Ok(headers)
}

pub(super) fn remove(project: &Path, args: McpRemoveArgs, policy: ExecutionPolicy) -> Result<()> {
    let _guard = super::begin(project, args.dry_run)?;
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
    let lock = Lockfile::load_optional(project)?;
    for (name, requirement) in manifest.mcp {
        let source = if requirement.server.is_some() {
            "registry"
        } else if requirement.url.is_some() {
            "remote"
        } else {
            "stdio"
        };
        let locked_transport = lock.as_ref().and_then(|lock| {
            lock.mcp_servers
                .iter()
                .find(|server| server.name == name)
                .and_then(|server| server.targets.first())
                .map(|target| target.transport.as_str())
        });
        let transport = locked_transport
            .or(requirement.transport.as_deref())
            .unwrap_or_else(|| {
                if requirement.command.is_some() {
                    "stdio"
                } else if requirement.url.is_some() {
                    "streamable-http"
                } else {
                    "unresolved"
                }
            });
        println!("{name}\t{source}\t{transport}");
    }
    Ok(())
}

pub(super) fn update(project: &Path, args: McpUpdateArgs, policy: ExecutionPolicy) -> Result<()> {
    let _guard = super::begin(project, args.dry_run)?;
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
    let projection = if args.no_sync {
        ProjectionPolicy::LockOnly
    } else {
        ProjectionPolicy::Project(if args.force {
            CollisionPolicy::Force
        } else {
            CollisionPolicy::Reject
        })
    };
    let request = policy
        .request(args.dry_run, projection)
        .with_updates(UpdateSelection::default().mcp(updates));
    super::execute(project, &manifest, request, policy.output)
}
