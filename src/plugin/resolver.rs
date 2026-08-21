use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::cache::Cache;
use crate::digest::canonical_json_digest;
use crate::error::{AruError, Result};
use crate::lockfile::{
    LockedSkill, Lockfile, PluginManifestRecord, PluginPackage, PluginSelection, ResourceOrigin,
    SkillPackage,
};
use crate::manifest::{
    Manifest, McpRequirement, PluginComponent, PluginRequirement, PluginTrust, Target,
};
use crate::source::git::{self, GitSource};

use super::{ADAPTER_VERSION, PluginInventory, inspect_plugin_root, plugin_root};

pub struct ResolveOptions<'a> {
    pub previous: Option<&'a Lockfile>,
    pub locked: bool,
    pub offline: bool,
    pub updates: &'a BTreeSet<String>,
    pub precise: &'a BTreeMap<String, String>,
}

pub struct PluginResolution {
    pub packages: Vec<PluginPackage>,
    pub skill_packages: Vec<SkillPackage>,
    pub skill_sources: BTreeMap<String, PathBuf>,
    pub mcp: BTreeMap<String, McpRequirement>,
    pub mcp_origins: BTreeMap<String, ResourceOrigin>,
}

pub struct PluginInspection {
    pub source: String,
    pub version: String,
    pub revision: String,
    pub inventory: PluginInventory,
}

pub fn inspect_source(
    project: &Path,
    raw_source: &str,
    requirement: &PluginRequirement,
    offline: bool,
    cache: &Cache,
) -> Result<PluginInspection> {
    let source = git::canonicalize(project, raw_source)?;
    if offline && !source.is_local() {
        return Err(AruError::msg(format!(
            "offline mode cannot inspect undeclared remote plugin {}",
            source.identity
        )));
    }
    let resolved = git::resolve(
        &source,
        requirement.version.as_deref(),
        requirement.branch.as_deref(),
        requirement.rev.as_deref(),
    )?;
    let checkout = cache.checkout_with_policy(&source, &resolved.revision, offline)?;
    let root = plugin_root(&checkout, requirement.subdir.as_deref())?;
    let inventory = inspect_plugin_root(&root, Some(requirement.format))?;
    Ok(PluginInspection {
        source: source.identity,
        version: resolved.version,
        revision: resolved.revision,
        inventory,
    })
}

pub fn resolve(
    project: &Path,
    manifest: &Manifest,
    cache: &Cache,
    options: ResolveOptions<'_>,
) -> Result<PluginResolution> {
    let mut packages = Vec::new();
    let mut skill_packages = Vec::new();
    let mut skill_sources = BTreeMap::new();
    let mut mcp = BTreeMap::new();
    let mut mcp_origins = BTreeMap::new();
    let mut resolved_names = BTreeSet::new();

    for (name, declared) in &manifest.plugins {
        let mut requirement = declared.clone();
        requirement.normalize();
        let source = git::canonicalize(project, &requirement.source)?;
        let old = options.previous.and_then(|lock| {
            lock.plugin_packages
                .iter()
                .find(|plugin| plugin.name == *name)
        });
        let resolved = git::select_reference(
            &source,
            reference(&requirement),
            old.map(|plugin| git::LockedReference {
                requirement: &plugin.requirement,
                version: &plugin.version,
                revision: &plugin.revision,
            }),
            git::ReferencePolicy {
                locked: options.locked,
                update: options.updates.contains(name),
                offline: options.offline,
                precise: options.precise.get(name).map(String::as_str),
                fallback_branch: Some("main"),
            },
            &format!("plugin {name:?}"),
        )?;
        let mut checkout =
            cache.checkout_with_policy(&source, &resolved.revision, options.offline)?;
        let mut root = plugin_root(&checkout, requirement.subdir.as_deref())?;
        let mut inventory = match inspect_plugin_root(&root, Some(requirement.format)) {
            Ok(inventory) => inventory,
            Err(error) if old.is_some_and(|old| old.revision == resolved.revision) => {
                cache.invalidate(&source, &resolved.revision)?;
                checkout =
                    cache.checkout_with_policy(&source, &resolved.revision, options.offline)?;
                root = plugin_root(&checkout, requirement.subdir.as_deref())?;
                inspect_plugin_root(&root, Some(requirement.format)).map_err(|_| error)?
            }
            Err(error) => return Err(error),
        };
        if inventory.name != *name {
            return Err(AruError::msg(format!(
                "plugin declaration name {name:?} does not match source name {:?}",
                inventory.name
            )));
        }
        if !resolved_names.insert(inventory.name.clone()) {
            return Err(AruError::msg(format!(
                "resolved plugin name {:?} is provided more than once",
                inventory.name
            )));
        }
        if inventory.format != requirement.format {
            return Err(AruError::msg(format!(
                "plugin {name:?} resolved as {}, not persisted format {}",
                inventory.format, requirement.format
            )));
        }
        if let Some(old) = old
            && old.revision == resolved.revision
            && (old.tree_sha256 != inventory.tree_sha256
                || old.manifests != manifest_records(&inventory))
        {
            cache.invalidate(&source, &resolved.revision)?;
            checkout = cache.checkout_with_policy(&source, &resolved.revision, options.offline)?;
            root = plugin_root(&checkout, requirement.subdir.as_deref())?;
            inventory = inspect_plugin_root(&root, Some(requirement.format))?;
            if old.tree_sha256 != inventory.tree_sha256
                || old.manifests != manifest_records(&inventory)
            {
                return Err(AruError::msg(format!(
                    "plugin {name:?} content for locked revision {} does not match aru.lock",
                    resolved.revision
                )));
            }
        }

        let targets = effective_targets(&requirement, &manifest.project.targets);
        let selection = select(name, &requirement, &inventory)?;
        validate_targets(
            name,
            &targets,
            !selection.skills.is_empty(),
            !selection.mcp.is_empty(),
        )?;
        let trust = manifest.plugin_trust.get(name);
        validate_trust(name, trust, &selection.mcp, &inventory)?;
        let origin = ResourceOrigin {
            kind: "plugin".into(),
            name: name.clone(),
            source: source.identity.clone(),
        };
        let locked_skills = selection
            .skills
            .iter()
            .map(|skill| LockedSkill {
                name: skill.name.clone(),
                path: skill.relative_path.clone(),
                sha256: skill.sha256.clone(),
                origin: Some(origin.clone()),
            })
            .collect::<Vec<_>>();
        for skill in &selection.skills {
            if skill_sources
                .insert(skill.name.clone(), skill.absolute_path.clone())
                .is_some()
            {
                return Err(AruError::msg(format!(
                    "resolved skill name {:?} is provided by more than one plugin",
                    skill.name
                )));
            }
        }
        if !locked_skills.is_empty() {
            skill_packages.push(SkillPackage {
                source: plugin_resource_source(name, &source, requirement.subdir.as_deref()),
                requirement: format!("plugin:{}", reference(&requirement).descriptor()),
                version: resolved.version.clone(),
                revision: resolved.revision.clone(),
                repository_name: name.clone(),
                targets: targets.clone(),
                skills: locked_skills.clone(),
            });
        }
        for selected in &selection.mcp {
            let mut normalized = selected.requirement.clone().ok_or_else(|| {
                AruError::msg(format!(
                    "plugin MCP {:?} is unsafe: {}",
                    selected.name,
                    selected
                        .issue
                        .as_deref()
                        .unwrap_or("unknown incompatibility")
                ))
            })?;
            normalized.targets = Some(targets.clone());
            if mcp.insert(selected.name.clone(), normalized).is_some() {
                return Err(AruError::msg(format!(
                    "resolved MCP name {:?} is provided by more than one plugin",
                    selected.name
                )));
            }
            mcp_origins.insert(selected.name.clone(), origin.clone());
        }
        packages.push(PluginPackage {
            name: name.clone(),
            source: source.identity,
            requirement: reference(&requirement).descriptor(),
            version: resolved.version,
            revision: resolved.revision,
            declared_version: inventory.declared_version.clone(),
            description: inventory.description.clone(),
            format: inventory.format,
            adapter_version: ADAPTER_VERSION,
            subdir: requirement.subdir.clone(),
            tree_sha256: inventory.tree_sha256.clone(),
            manifests: manifest_records(&inventory),
            selection: PluginSelection {
                whole: requirement.whole_plugin(),
                components: requirement.components.clone(),
                skills: requirement.skills.clone(),
                mcp: requirement.mcp.clone(),
            },
            targets,
            skills: locked_skills,
            mcp: selection
                .mcp
                .iter()
                .map(|server| server.name.clone())
                .collect(),
            unsupported: inventory.unsupported.clone(),
            diagnostics: inventory_diagnostics(&inventory),
        });
    }
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    skill_packages.sort_by(|left, right| left.source.cmp(&right.source));
    if options.locked
        && options
            .previous
            .is_none_or(|lock| lock.plugin_packages != packages)
    {
        return Err(AruError::msg("aru.lock is stale for plugin packages"));
    }
    Ok(PluginResolution {
        packages,
        skill_packages,
        skill_sources,
        mcp,
        mcp_origins,
    })
}

pub fn preflight(
    name: &str,
    requirement: &PluginRequirement,
    trust: Option<&PluginTrust>,
    project_targets: &[Target],
    inventory: &PluginInventory,
) -> Result<()> {
    let targets = effective_targets(requirement, project_targets);
    let selection = select(name, requirement, inventory)?;
    validate_targets(
        name,
        &targets,
        !selection.skills.is_empty(),
        !selection.mcp.is_empty(),
    )?;
    validate_trust(name, trust, &selection.mcp, inventory)
}

struct Selected<'a> {
    skills: Vec<&'a super::InventorySkill>,
    mcp: Vec<&'a super::InventoryMcp>,
}

fn select<'a>(
    name: &str,
    requirement: &PluginRequirement,
    inventory: &'a PluginInventory,
) -> Result<Selected<'a>> {
    let whole = requirement.whole_plugin();
    if whole {
        let mut blockers = inventory.unsupported.clone();
        blockers.extend(
            inventory
                .diagnostics
                .iter()
                .filter(|item| {
                    item.starts_with("invalid ")
                        || item.starts_with("disabled invalid")
                        || item.starts_with("skipped invalid")
                })
                .cloned(),
        );
        blockers.extend(inventory.mcp.iter().filter_map(|server| {
            server
                .issue
                .as_ref()
                .map(|issue| format!("mcp:{}: {issue}", server.name))
        }));
        if !blockers.is_empty() {
            blockers.sort();
            return Err(AruError::msg(format!(
                "plugin {name:?} whole-plugin selection contains unsupported capabilities: {}; add explicit --component/--skill/--mcp selectors",
                blockers.join(", ")
            )));
        }
        return Ok(Selected {
            skills: inventory.skills.iter().collect(),
            mcp: inventory.mcp.iter().collect(),
        });
    }

    let skills = if requirement.components.contains(&PluginComponent::Skills) {
        inventory.skills.iter().collect()
    } else {
        select_names(
            name,
            "skill",
            &requirement.skills,
            &inventory.skills,
            |skill| skill.name.as_str(),
        )?
    };
    let mcp = if requirement.components.contains(&PluginComponent::Mcp) {
        let unsafe_servers = inventory
            .mcp
            .iter()
            .filter_map(|server| server.issue.as_ref().map(|issue| (&server.name, issue)))
            .collect::<Vec<_>>();
        if !unsafe_servers.is_empty() {
            return Err(AruError::msg(format!(
                "plugin {name:?} whole-MCP selection contains unsafe entries: {}",
                unsafe_servers
                    .iter()
                    .map(|(name, issue)| format!("{name}: {issue}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        inventory.mcp.iter().collect()
    } else {
        select_names(name, "MCP", &requirement.mcp, &inventory.mcp, |server| {
            server.name.as_str()
        })?
    };
    for server in &mcp {
        if let Some(issue) = &server.issue {
            return Err(AruError::msg(format!(
                "plugin MCP {:?} is unsafe: {issue}",
                server.name
            )));
        }
    }
    Ok(Selected { skills, mcp })
}

fn select_names<'a, T>(
    plugin: &str,
    kind: &str,
    names: &[String],
    values: &'a [T],
    key: impl Fn(&T) -> &str,
) -> Result<Vec<&'a T>> {
    let mut output = Vec::new();
    for name in names {
        let value = values
            .iter()
            .find(|value| key(value) == name)
            .ok_or_else(|| {
                let available = values.iter().map(&key).collect::<Vec<_>>().join(", ");
                AruError::msg(format!(
                    "plugin {plugin:?} has no {kind} {name:?}; available: {available}"
                ))
            })?;
        output.push(value);
    }
    Ok(output)
}

fn validate_trust(
    plugin: &str,
    trust: Option<&PluginTrust>,
    selected: &[&super::InventoryMcp],
    inventory: &PluginInventory,
) -> Result<()> {
    let trusted = trust
        .map(|trust| {
            trust
                .mcp
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    for server in selected {
        if !trusted.contains(server.name.as_str()) {
            return Err(AruError::msg(format!(
                "untrusted plugin MCP {:?} from {plugin:?}; add `--trust-mcp {}` or [plugin-trust.{plugin}] mcp = [\"{}\"]",
                server.name, server.name, server.name
            )));
        }
    }
    for name in trusted {
        if !inventory.mcp.iter().any(|server| server.name == name) {
            return Err(AruError::msg(format!(
                "plugin trust {plugin:?} names unknown MCP {name:?}"
            )));
        }
        if !selected.iter().any(|server| server.name == name) {
            return Err(AruError::msg(format!(
                "plugin trust {plugin:?} names unselected MCP {name:?}"
            )));
        }
    }
    Ok(())
}

fn effective_targets(requirement: &PluginRequirement, project_targets: &[Target]) -> Vec<Target> {
    let mut targets = requirement
        .targets
        .clone()
        .unwrap_or_else(|| project_targets.to_vec());
    targets.sort();
    targets.dedup();
    targets
}

fn validate_targets(plugin: &str, targets: &[Target], skills: bool, mcp: bool) -> Result<()> {
    if skills
        && targets
            .iter()
            .any(|target| !crate::target::capabilities(*target).skills)
    {
        return Err(AruError::msg(format!(
            "plugin {plugin:?} selects skills unsupported by an effective target; narrow --target"
        )));
    }
    if mcp
        && targets
            .iter()
            .any(|target| !crate::target::capabilities(*target).mcp)
    {
        return Err(AruError::msg(format!(
            "plugin {plugin:?} selects MCP unsupported by an effective target; narrow --target"
        )));
    }
    Ok(())
}

fn inventory_diagnostics(inventory: &PluginInventory) -> Vec<String> {
    let mut diagnostics = inventory.diagnostics.clone();
    diagnostics.extend(inventory.mcp.iter().filter_map(|server| {
        server
            .issue
            .as_ref()
            .map(|issue| format!("mcp:{}: {issue}", server.name))
    }));
    diagnostics.sort();
    diagnostics.dedup();
    diagnostics
}

fn manifest_records(inventory: &PluginInventory) -> Vec<PluginManifestRecord> {
    inventory
        .manifests
        .iter()
        .map(|manifest| PluginManifestRecord {
            path: manifest.path.clone(),
            sha256: manifest.sha256.clone(),
        })
        .collect()
}

fn reference(requirement: &PluginRequirement) -> git::ReferenceSpec<'_> {
    git::ReferenceSpec::new(
        requirement.version.as_deref(),
        requirement.branch.as_deref(),
        requirement.rev.as_deref(),
    )
}

fn plugin_resource_source(name: &str, source: &GitSource, subdir: Option<&str>) -> String {
    format!(
        "plugin+{}#{}:{}",
        source.identity,
        subdir.unwrap_or("."),
        name
    )
}

pub fn package_input_descriptor(
    project: &Path,
    manifest: &Manifest,
) -> Result<Vec<(String, String, PluginRequirement, PluginTrust)>> {
    let mut output = Vec::new();
    for (name, requirement) in &manifest.plugins {
        let source = git::canonicalize(project, &requirement.source)?;
        let mut requirement = requirement.clone();
        requirement.normalize();
        requirement.source = source.identity.clone();
        requirement.targets = None;
        let mut trust = manifest.plugin_trust.get(name).cloned().unwrap_or_default();
        trust.normalize();
        output.push((name.clone(), source.identity, requirement, trust));
    }
    output.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(output)
}

pub fn input_digest(project: &Path, manifest: &Manifest) -> Result<String> {
    canonical_json_digest(&package_input_descriptor(project, manifest)?)
}
