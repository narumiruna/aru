use std::collections::BTreeSet;
use std::path::Path;

use crate::cli::{McpAddArgs, McpRemoveArgs, McpUpdateArgs};
use crate::error::{AruError, Result};
use crate::lockfile::Lockfile;
use crate::manifest::{ManifestDocument, McpRequirement, validate_name};

use super::ExecutionPolicy;

pub(super) fn add(project: &Path, args: McpAddArgs, policy: ExecutionPolicy) -> Result<()> {
    let _guard = super::begin(project, args.dry_run)?;
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
        command: args.command,
        args: args.args,
        bearer_token_env: args.bearer_token_env,
    };
    requirement.validate(&args.name)?;
    document.set_mcp(&args.name, &requirement);
    let manifest = document.manifest()?;
    super::execute(
        project,
        &manifest,
        Some(document.bytes()),
        args.dry_run,
        policy,
        !args.no_sync,
        false,
        args.force,
        BTreeSet::new(),
        BTreeSet::new(),
    )
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
    super::execute(
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
    super::execute(
        project,
        &manifest,
        None,
        args.dry_run,
        policy,
        !args.no_sync,
        false,
        args.force,
        BTreeSet::new(),
        updates,
    )
}
