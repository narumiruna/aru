use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::cache::Cache;
use crate::cli::{PluginAddArgs, PluginInfoArgs, PluginRemoveArgs, PluginUpdateArgs};
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::Lockfile;
use crate::manifest::{
    ManifestDocument, PluginComponent, PluginRequirement, PluginTrust, validate_name,
    validate_plugin_name,
};
use crate::plugin::{inspect_plugin_root, plugin_root};
use crate::source::git;
use crate::sync::{CollisionPolicy, UpdateSelection};

use super::{ExecutionPolicy, ProjectionPolicy, begin, execute};

pub(super) fn add(project: &Path, args: PluginAddArgs, policy: ExecutionPolicy) -> Result<()> {
    validate_cli_selection(&args)?;
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;

    let source = git::canonicalize(project, &args.source)?;
    let spec = git::ReferenceSpec::new(
        args.version.as_deref(),
        args.branch.as_deref(),
        args.rev.as_deref(),
    );
    let resolved = git::select_reference(
        &source,
        spec,
        None,
        git::ReferencePolicy {
            offline: policy.offline,
            fallback_branch: Some("main"),
            ..git::ReferencePolicy::default()
        },
        "plugin source",
    )?;
    let cache = Cache::ephemeral_for_project(project)?;
    let checkout = cache.checkout_with_policy(&source, &resolved.revision, policy.offline)?;
    let root = plugin_root(&checkout, args.subdir.as_deref())?;
    let inventory = inspect_plugin_root(&root, args.format)?;
    let name = inventory.name.clone();
    validate_plugin_name(&name)?;
    if manifest.plugins.contains_key(&name) {
        return Err(AruError::msg(format!(
            "plugin {name:?} is already configured; use `aru plugin update {name}` or edit aru.toml selections and run `aru sync`"
        )));
    }

    let mut requirement = PluginRequirement {
        source: args.source,
        format: inventory.format,
        subdir: args.subdir,
        version: args.version,
        branch: args.branch,
        rev: args.rev,
        components: args.components,
        skills: args.skills,
        mcp: args.mcp,
        targets: (!args.targets.is_empty()).then_some(args.targets),
    };
    requirement.normalize();
    requirement.validate(&name, &manifest.project.targets)?;
    let trust = if args.trust_mcp.is_empty() {
        None
    } else {
        let mut trust = PluginTrust {
            mcp: args.trust_mcp,
        };
        trust.normalize();
        trust.validate(&name)?;
        Some(trust)
    };
    crate::plugin::resolver::preflight(
        &name,
        &requirement,
        trust.as_ref(),
        &manifest.project.targets,
        &inventory,
    )?;
    document.set_plugin(&name, &requirement);
    if let Some(trust) = trust {
        document.set_plugin_trust(&name, &trust);
    }
    let manifest = document.manifest()?;
    let request = policy
        .request(args.dry_run, projection(args.no_sync, args.force))
        .with_manifest_bytes(document.bytes());
    execute(project, &manifest, request, policy.output)
}

pub(super) fn remove(
    project: &Path,
    args: PluginRemoveArgs,
    policy: ExecutionPolicy,
) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let mut document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    if !manifest.plugins.contains_key(&args.name) {
        return Err(AruError::msg(format!(
            "plugin {:?} is not configured",
            args.name
        )));
    }
    document.remove_plugin(&args.name);
    document.remove_plugin_trust(&args.name);
    let manifest = document.manifest()?;
    let request = policy
        .request(args.dry_run, projection(args.no_sync, false))
        .with_manifest_bytes(document.bytes());
    execute(project, &manifest, request, policy.output)
}

pub(super) fn update(
    project: &Path,
    args: PluginUpdateArgs,
    policy: ExecutionPolicy,
) -> Result<()> {
    let _guard = begin(project, args.dry_run)?;
    let document = ManifestDocument::load(project)?;
    let manifest = document.manifest()?;
    if manifest.plugins.is_empty() {
        return Err(AruError::msg("aru.toml declares no plugins"));
    }
    let updates = if args.names.is_empty() {
        manifest.plugins.keys().cloned().collect::<BTreeSet<_>>()
    } else {
        let mut selected = BTreeSet::new();
        for name in args.names {
            if !manifest.plugins.contains_key(&name) {
                return Err(AruError::msg(format!("plugin {name:?} is not configured")));
            }
            selected.insert(name);
        }
        selected
    };
    let precise = if let Some(version) = args.precise {
        if updates.len() != 1 {
            return Err(AruError::msg(
                "--precise requires exactly one selected plugin",
            ));
        }
        BTreeMap::from([(updates.iter().next().unwrap().clone(), version)])
    } else {
        BTreeMap::new()
    };
    let request = policy
        .request(args.dry_run, projection(args.no_sync, args.force))
        .with_updates(UpdateSelection::default().plugins(updates, precise));
    execute(project, &manifest, request, policy.output)
}

pub(super) fn list(project: &Path) -> Result<()> {
    let manifest = ManifestDocument::load(project)?.manifest()?;
    let lock = Lockfile::load_optional(project)?;
    for (name, requirement) in manifest.plugins {
        if let Some(plugin) = lock.as_ref().and_then(|lock| {
            lock.plugin_packages
                .iter()
                .find(|plugin| plugin.name == name)
        }) {
            println!(
                "{}\t{}\t{}\t{}",
                name, plugin.version, plugin.format, plugin.source
            );
        } else {
            println!(
                "{}\tunlocked\t{}\t{}",
                name, requirement.format, requirement.source
            );
        }
    }
    Ok(())
}

pub(super) fn info(project: &Path, args: PluginInfoArgs, policy: ExecutionPolicy) -> Result<()> {
    let project_targets = ManifestDocument::load(project)?.manifest()?.project.targets;
    if let Some(plugin) = Lockfile::load_optional(project)?.and_then(|lock| {
        lock.plugin_packages
            .into_iter()
            .find(|plugin| plugin.name == args.source)
    }) {
        print_locked(&plugin)?;
        return Ok(());
    }

    let local = project.join(&args.source);
    let local = if local.is_dir() {
        Some(local.canonicalize().at(&local)?)
    } else {
        let path = Path::new(&args.source);
        path.is_absolute()
            .then(|| path.canonicalize().at(path))
            .transpose()?
            .filter(|path| path.is_dir())
    };
    if let Some(local) = local
        && has_plugin_manifest(&local, args.subdir.as_deref())
    {
        let root = plugin_root(&local, args.subdir.as_deref())?;
        let inventory = inspect_plugin_root(&root, args.format)?;
        print_inventory(&inventory, None, None, &project_targets)?;
        return Ok(());
    }

    let source = git::canonicalize(project, &args.source)?;
    let resolved = git::select_reference(
        &source,
        git::ReferenceSpec::new(
            args.version.as_deref(),
            args.branch.as_deref(),
            args.rev.as_deref(),
        ),
        None,
        git::ReferencePolicy {
            offline: policy.offline,
            fallback_branch: Some("main"),
            ..git::ReferencePolicy::default()
        },
        "plugin source",
    )?;
    let cache = Cache::ephemeral_for_project(project)?;
    let checkout = cache.checkout_with_policy(&source, &resolved.revision, policy.offline)?;
    let root = plugin_root(&checkout, args.subdir.as_deref())?;
    let inventory = inspect_plugin_root(&root, args.format)?;
    print_inventory(
        &inventory,
        Some((&source.identity, &resolved.version)),
        Some(&resolved.revision),
        &project_targets,
    )
}

fn print_locked(plugin: &crate::lockfile::PluginPackage) -> Result<()> {
    println!("name:         {}", plugin.name);
    println!("locked:       {}", plugin.version);
    println!("format:       {}", plugin.format);
    if let Some(description) = &plugin.description {
        println!("description:  {}", single_line(description));
    }
    println!(
        "source:       {}",
        crate::export::scrub_url(&plugin.source, "plugin source URL")?
    );
    println!("revision:     {}", plugin.revision);
    println!("manifests:    {}", plugin.manifests.len());
    println!("skills:       {}", plugin.skills.len());
    println!("mcp:          {}", plugin.mcp.len());
    println!("unsupported:  {}", plugin.unsupported.len());
    println!("diagnostics:  {}", plugin.diagnostics.len());
    println!(
        "targets:      {}",
        plugin
            .targets
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}

fn print_inventory(
    inventory: &crate::plugin::PluginInventory,
    source: Option<(&str, &str)>,
    revision: Option<&str>,
    project_targets: &[crate::manifest::Target],
) -> Result<()> {
    println!("name:         {}", inventory.name);
    println!("format:       {}", inventory.format);
    if let Some(version) = &inventory.declared_version {
        println!("declared:     {version}");
    }
    if let Some(description) = &inventory.description {
        println!("description:  {}", single_line(description));
    }
    if let Some((source, version)) = source {
        println!("available:    {version}");
        println!(
            "source:       {}",
            crate::export::scrub_url(source, "plugin source URL")?
        );
    }
    if let Some(revision) = revision {
        println!("revision:     {revision}");
    }
    println!("manifests:    {}", inventory.manifests.len());
    println!("skills:       {}", inventory.skills.len());
    println!("mcp:          {}", inventory.mcp.len());
    let compatible = project_targets
        .iter()
        .filter(|target| {
            (inventory.skills.is_empty() || crate::target::capabilities(**target).skills)
                && (inventory.mcp.is_empty() || crate::target::capabilities(**target).mcp)
        })
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    println!("compatible:   {compatible}");
    println!("unsupported:  {}", inventory.unsupported.len());
    let addable = inventory.unsupported.is_empty()
        && inventory.mcp.iter().all(|server| server.issue.is_none())
        && inventory.diagnostics.iter().all(|diagnostic| {
            !diagnostic.starts_with("invalid ")
                && !diagnostic.starts_with("disabled invalid")
                && !diagnostic.starts_with("skipped invalid")
        });
    println!("whole addable: {}", if addable { "yes" } else { "no" });
    for item in &inventory.unsupported {
        println!("  unsupported {item}");
    }
    for server in &inventory.mcp {
        if let Some(issue) = &server.issue {
            println!("  unsafe MCP {}: {issue}", server.name);
        } else {
            println!("  trust required for MCP {}", server.name);
        }
    }
    Ok(())
}

fn single_line(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(256)
        .collect()
}

fn has_plugin_manifest(root: &Path, subdir: Option<&str>) -> bool {
    let selected = subdir.map_or_else(|| root.to_path_buf(), |subdir| root.join(subdir));
    selected.join("plugin.json").is_file()
        || selected.join(".codex-plugin/plugin.json").is_file()
        || selected.join("gemini-extension.json").is_file()
}

fn validate_cli_selection(args: &PluginAddArgs) -> Result<()> {
    if args.components.contains(&PluginComponent::Skills) && !args.skills.is_empty() {
        return Err(AruError::msg("--component skills conflicts with --skill"));
    }
    if args.components.contains(&PluginComponent::Mcp) && !args.mcp.is_empty() {
        return Err(AruError::msg("--component mcp conflicts with --mcp"));
    }
    for name in args
        .skills
        .iter()
        .chain(args.mcp.iter())
        .chain(args.trust_mcp.iter())
    {
        validate_name(name, "plugin resource name")?;
    }
    Ok(())
}

fn projection(no_sync: bool, force: bool) -> ProjectionPolicy {
    if no_sync {
        ProjectionPolicy::LockOnly
    } else if force {
        ProjectionPolicy::Project(CollisionPolicy::Force)
    } else {
        ProjectionPolicy::Project(CollisionPolicy::Reject)
    }
}
