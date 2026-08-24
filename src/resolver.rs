mod skill;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cache::Cache;
use crate::digest::canonical_json_digest;
use crate::error::{AruError, Result};
use crate::instruction::{self, DiscoveredInstruction};
use crate::lockfile::{
    ADAPTER_CAPABILITY_SCHEMA, Lockfile, McpServer, McpTarget, ProjectionBaseline,
};
use crate::manifest::{Manifest, McpRequirement, PackageRequirement, SkillRequirement, Target};
use crate::registry::{RegistryClient, ResolvedCandidate};
use crate::source::git::{self, GitSource};
use crate::target;

pub use skill::{
    SkillResolutionHint, SkillSourceInspection, canonical_update_skill_targets,
    inspect_skill_source,
};
pub(crate) use skill::{declared_skill_source_key, inspect_skill_source_with_cache};

#[derive(Debug)]
pub struct Resolution {
    pub lock: Lockfile,
    pub skill_sources: BTreeMap<String, PathBuf>,
    pub instructions: Vec<DiscoveredInstruction>,
}

pub struct ResolveOptions<'a> {
    pub previous: Option<&'a Lockfile>,
    pub locked: bool,
    pub offline: bool,
    pub materialize_skills: bool,
    pub update_skills: &'a BTreeSet<String>,
    pub update_mcp: &'a BTreeSet<String>,
    pub update_packages: &'a BTreeSet<String>,
    pub precise_packages: &'a BTreeMap<String, String>,
    pub dry_run: bool,
    pub skill_hints: &'a BTreeMap<String, SkillResolutionHint>,
}

pub fn resolve(
    project: &Path,
    manifest: &Manifest,
    options: ResolveOptions<'_>,
) -> Result<Resolution> {
    let skill_sources = skill::SkillSourceCatalog::canonicalize(project, &manifest.skills)?;
    let package_sources = canonical_package_sources(project, manifest)?;
    for source in skill_sources.values() {
        if package_sources
            .values()
            .any(|package| package.identity == source.identity)
        {
            return Err(AruError::msg(format!(
                "Git source {} cannot be both a direct skill source and an aru package",
                source.identity
            )));
        }
    }
    let mut instructions = instruction::discovery::discover(project, manifest)?;
    skill::validate_project_targets(manifest)?;
    if !manifest.mcp.is_empty() && target::mcp_targets(&manifest.project.targets).is_empty() {
        return Err(AruError::msg(
            "no configured target supports MCP projections",
        ));
    }
    let legacy_v3 = options.locked && options.previous.is_some_and(|lock| lock.version == 3);
    let package_input_hash = package_input_hash_at(
        project,
        manifest,
        skill_sources.as_map(),
        &package_sources,
        legacy_v3,
    )?;
    let locked = if options.locked {
        let lock = options
            .previous
            .cloned()
            .ok_or_else(|| AruError::msg("--locked requires an existing aru.lock"))?;
        if lock.package_input_hash != package_input_hash {
            return Err(AruError::msg(
                "aru.lock is stale for package inputs; run aru lock or aru sync",
            ));
        }
        Some(lock)
    } else {
        None
    };
    let cache = if options.dry_run {
        Cache::ephemeral_for_project(project)?
    } else {
        Cache::project(project)
    };
    let package_resolution = crate::package::resolver::resolve(
        project,
        manifest,
        &cache,
        crate::package::resolver::ResolveOptions {
            previous: options.previous,
            locked: options.locked,
            offline: options.offline,
            update: options.update_packages,
            precise: options.precise_packages,
        },
    )?;
    let plugin_resolution = crate::plugin::resolver::resolve(
        project,
        manifest,
        &cache,
        crate::plugin::resolver::ResolveOptions {
            previous: options.previous,
            locked: options.locked,
            offline: options.offline,
            updates: options.update_packages,
            precise: options.precise_packages,
        },
    )?;
    instructions.extend(package_resolution.instructions.clone());
    instructions.sort_by(|left, right| left.unit.source.cmp(&right.unit.source));
    let mut combined_mcp = manifest.mcp.clone();
    for (name, requirement) in &package_resolution.mcp {
        if combined_mcp
            .insert(name.clone(), requirement.clone())
            .is_some()
        {
            return Err(AruError::msg(format!(
                "MCP name {name:?} is provided by both the project and an aru package"
            )));
        }
    }
    for (name, requirement) in &plugin_resolution.mcp {
        if combined_mcp
            .insert(name.clone(), requirement.clone())
            .is_some()
        {
            return Err(AruError::msg(format!(
                "MCP name {name:?} is provided by a plugin and another source"
            )));
        }
    }

    if let Some(lock) = locked {
        instruction::lock::validate_locked_sources(&lock.instruction_sources, &instructions)?;
        for expected in package_resolution
            .skill_packages
            .iter()
            .chain(plugin_resolution.skill_packages.iter())
        {
            if !lock.skill_packages.contains(expected) {
                return Err(AruError::msg(
                    "aru.lock is stale for a package or plugin skill export",
                ));
            }
        }
        let mut effective_manifest = manifest.clone();
        effective_manifest.mcp = combined_mcp;
        validate_locked_mcp(&lock, &effective_manifest.mcp, &manifest.project.targets)?;
        skill::validate_locked_targets(&lock, manifest, &skill_sources)?;
        validate_locked_projection(&lock, &effective_manifest, &instructions)?;
        let mut materialized_skills = skill::locked_sources(
            &cache,
            manifest,
            &skill_sources,
            &lock,
            options.materialize_skills,
            options.offline,
        )?;
        for (name, path) in package_resolution
            .skill_sources
            .into_iter()
            .chain(plugin_resolution.skill_sources)
        {
            if materialized_skills.insert(name.clone(), path).is_some() {
                return Err(AruError::msg(format!(
                    "resolved skill name {name:?} is provided more than once"
                )));
            }
        }
        return Ok(Resolution {
            lock,
            skill_sources: materialized_skills,
            instructions,
        });
    }

    let previous = options.previous;
    let direct_skills = skill::resolve(
        manifest,
        &skill_sources,
        &cache,
        skill::DirectSkillOptions {
            previous,
            offline: options.offline,
            updates: options.update_skills,
            hints: options.skill_hints,
        },
    )?;
    let mut skill_packages = package_resolution.skill_packages;
    skill_packages.extend(plugin_resolution.skill_packages);
    skill_packages.extend(direct_skills.packages);
    let mut materialized_skills = package_resolution.skill_sources;
    for (name, path) in plugin_resolution.skill_sources {
        if materialized_skills.insert(name.clone(), path).is_some() {
            return Err(AruError::msg(format!(
                "resolved skill name {name:?} is provided by more than one package or plugin"
            )));
        }
    }
    for (name, path) in direct_skills.sources {
        if materialized_skills.insert(name.clone(), path).is_some() {
            return Err(AruError::msg(format!(
                "resolved skill name {name:?} is provided by more than one package"
            )));
        }
    }

    let registry_client = RegistryClient::new()?;
    let mut mcp_servers = Vec::new();
    for (name, requirement) in &combined_mcp {
        let descriptor = canonical_json_digest(&normalized_mcp(requirement))?;
        let targets =
            effective_mcp_targets(&manifest.project.targets, requirement.targets.as_deref());
        let old =
            previous.and_then(|lock| lock.mcp_servers.iter().find(|server| server.name == *name));
        let reusable = old.filter(|server| {
            !options.update_mcp.contains(name) && server.requirement == descriptor
        });
        let mut server = if let Some(old) = reusable {
            rebuild_mcp_targets(old, requirement, &targets)?
        } else {
            resolve_mcp(
                &registry_client,
                name,
                requirement,
                &targets,
                &descriptor,
                options.offline,
            )?
        };
        server.origin = plugin_resolution.mcp_origins.get(name).cloned();
        mcp_servers.push(server);
    }

    let mut lock = Lockfile {
        version: 4,
        package_input_hash,
        projection_input_hash: String::new(),
        instruction_sources: instruction::lock::locked_sources(&instructions),
        aru_packages: package_resolution.packages,
        skill_packages,
        mcp_servers,
        plugin_packages: plugin_resolution.packages,
        projection_baselines: Vec::new(),
    };
    lock.normalize();
    lock.projection_baselines = baselines(&lock, &instructions)?;
    lock.projection_input_hash = projection_input_hash(&lock, &manifest.project.targets)?;
    lock.normalize();
    lock.validate()?;
    Ok(Resolution {
        lock,
        skill_sources: materialized_skills,
        instructions,
    })
}

fn effective_mcp_targets(project_targets: &[Target], selected: Option<&[Target]>) -> Vec<Target> {
    let mut targets = selected.unwrap_or(project_targets).to_vec();
    targets.retain(|target| target::capabilities(*target).mcp);
    targets.sort();
    targets.dedup();
    targets
}

fn canonical_package_sources(
    project: &Path,
    manifest: &Manifest,
) -> Result<BTreeMap<String, GitSource>> {
    let mut output = BTreeMap::new();
    let mut identities = BTreeMap::<String, String>::new();
    for source in manifest.packages.keys() {
        let canonical = git::canonicalize(project, source)?;
        if let Some(previous) = identities.insert(canonical.identity.clone(), source.clone()) {
            return Err(AruError::msg(format!(
                "aru package sources {previous:?} and {source:?} identify the same repository"
            )));
        }
        output.insert(source.clone(), canonical);
    }
    Ok(output)
}

#[derive(Serialize)]
struct PackageInputs {
    packages: Vec<PackageInput>,
    skills: Vec<SkillInput>,
    mcp: BTreeMap<String, McpRequirement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    plugin_input: Option<String>,
}

#[derive(Serialize)]
struct PackageInput {
    source: String,
    requirement: PackageRequirement,
}

#[derive(Serialize)]
struct SkillInput {
    source: String,
    requirement: SkillRequirement,
}

fn package_input_hash_at(
    project: &Path,
    manifest: &Manifest,
    sources: &BTreeMap<String, GitSource>,
    package_sources: &BTreeMap<String, GitSource>,
    legacy_v3: bool,
) -> Result<String> {
    let mut packages = manifest
        .packages
        .iter()
        .map(|(key, requirement)| {
            let mut requirement = requirement.clone();
            requirement.normalize();
            requirement.targets = None;
            if requirement.rev.is_none() && requirement.branch.is_none() {
                requirement.version = Some(normalize_semver_requirement(
                    requirement.version.as_deref().unwrap_or("*"),
                ));
            }
            PackageInput {
                source: package_sources.get(key).unwrap().identity.clone(),
                requirement,
            }
        })
        .collect::<Vec<_>>();
    packages.sort_by(|left, right| left.source.cmp(&right.source));
    let mut skills: Vec<_> = manifest
        .skills
        .iter()
        .map(|(key, requirement)| {
            let mut requirement = requirement.clone();
            requirement.normalize();
            requirement.targets = None;
            if requirement.rev.is_none() && requirement.branch.is_none() {
                requirement.version = Some(normalize_semver_requirement(
                    requirement.version.as_deref().unwrap_or("*"),
                ));
            }
            SkillInput {
                source: sources.get(key).unwrap().identity.clone(),
                requirement,
            }
        })
        .collect();
    skills.sort_by(|left, right| left.source.cmp(&right.source));
    let mcp = manifest
        .mcp
        .iter()
        .map(|(name, requirement)| (name.clone(), normalized_mcp(requirement)))
        .collect();
    canonical_json_digest(&PackageInputs {
        packages,
        skills,
        mcp,
        plugin_input: if legacy_v3 {
            None
        } else {
            Some(crate::plugin::resolver::input_digest(project, manifest)?)
        },
    })
}

#[cfg(test)]
fn package_input_hash(
    manifest: &Manifest,
    sources: &BTreeMap<String, GitSource>,
    package_sources: &BTreeMap<String, GitSource>,
) -> Result<String> {
    package_input_hash_at(Path::new("."), manifest, sources, package_sources, false)
}

fn normalized_mcp(requirement: &McpRequirement) -> McpRequirement {
    let mut normalized = requirement.clone();
    normalized.normalize();
    normalized.targets = None;
    if normalized.server.is_some() {
        if normalized.registry.is_none() {
            normalized.registry = Some(crate::registry::DEFAULT_REGISTRY.into());
        }
        normalized.version = Some(normalize_semver_requirement(
            normalized.version.as_deref().unwrap_or("*"),
        ));
    } else if normalized.command.is_some() && normalized.transport.is_none() {
        normalized.transport = Some("stdio".into());
    } else if normalized.url.is_some() && normalized.transport.is_none() {
        normalized.transport = Some("streamable-http".into());
    }
    normalized
}

fn normalize_semver_requirement(requirement: &str) -> String {
    git::normalize_version_requirement(requirement)
}

fn resolve_mcp(
    client: &RegistryClient,
    name: &str,
    requirement: &McpRequirement,
    targets: &[Target],
    descriptor: &str,
    offline: bool,
) -> Result<McpServer> {
    let (server_id, registry, version, metadata_sha256, candidate) =
        if let Some(command) = &requirement.command {
            let candidate = ResolvedCandidate {
                kind: "command".into(),
                transport: requirement
                    .transport
                    .clone()
                    .unwrap_or_else(|| "stdio".into()),
                command: Some(command.clone()),
                args: requirement.args.clone(),
                env_vars: {
                    let mut env_vars = requirement.env_vars.clone();
                    env_vars.sort();
                    env_vars
                },
                env_http_headers: BTreeMap::new(),
                bearer_token_env: None,
                url: None,
                package: None,
            };
            (
                name.to_owned(),
                None,
                "direct".to_owned(),
                canonical_json_digest(&candidate)?,
                candidate,
            )
        } else if let Some(url) = &requirement.url {
            let candidate = ResolvedCandidate {
                kind: "remote".into(),
                transport: requirement
                    .transport
                    .clone()
                    .unwrap_or_else(|| "streamable-http".into()),
                command: None,
                args: Vec::new(),
                env_vars: Vec::new(),
                env_http_headers: requirement.env_http_headers.clone(),
                bearer_token_env: requirement.bearer_token_env.clone(),
                url: Some(url.clone()),
                package: None,
            };
            (
                name.to_owned(),
                None,
                "direct".to_owned(),
                canonical_json_digest(&candidate)?,
                candidate,
            )
        } else {
            if offline {
                return Err(AruError::msg(format!(
                    "offline mode cannot resolve MCP Registry server {name:?}"
                )));
            }
            let resolution = client.resolve(requirement, targets)?;
            (
                requirement.server.clone().unwrap(),
                Some(
                    requirement
                        .registry
                        .clone()
                        .unwrap_or_else(|| crate::registry::DEFAULT_REGISTRY.into()),
                ),
                resolution.version,
                resolution.metadata_sha256,
                resolution.candidate,
            )
        };
    let projections =
        targets_from_candidate(targets, &candidate, requirement.bearer_token_env.as_ref())?;
    Ok(McpServer {
        name: name.into(),
        origin: None,
        registry,
        server_id,
        requirement: descriptor.into(),
        version,
        metadata_sha256,
        targets: projections,
    })
}

fn targets_from_candidate(
    targets: &[Target],
    candidate: &ResolvedCandidate,
    bearer_token_env: Option<&String>,
) -> Result<Vec<McpTarget>> {
    if bearer_token_env.is_some() && candidate.transport != "streamable-http" {
        return Err(AruError::msg(
            "bearer-token-env can only be used with streamable-http MCP servers",
        ));
    }
    let mut projections = Vec::new();
    for target in targets {
        if !crate::registry::supports(target, candidate) {
            return Err(AruError::msg(format!(
                "MCP candidate transport {} is unsupported by {target}",
                candidate.transport
            )));
        }
        let projection = McpTarget {
            target: *target,
            kind: candidate.kind.clone(),
            transport: candidate.transport.clone(),
            command: candidate.command.clone(),
            args: candidate.args.clone(),
            env_vars: candidate.env_vars.clone(),
            env_http_headers: candidate.env_http_headers.clone(),
            url: candidate.url.clone(),
            bearer_token_env: bearer_token_env
                .cloned()
                .or_else(|| candidate.bearer_token_env.clone()),
            package: candidate.package.clone(),
        };
        target::normalized_entry(&projection)?;
        projections.push(projection);
    }
    projections.sort_by_key(|projection| projection.target);
    Ok(projections)
}

fn rebuild_mcp_targets(
    old: &McpServer,
    requirement: &McpRequirement,
    targets: &[Target],
) -> Result<McpServer> {
    let first = old
        .targets
        .first()
        .ok_or_else(|| AruError::msg("locked MCP server has no target selection"))?;
    let candidate = ResolvedCandidate {
        kind: first.kind.clone(),
        transport: first.transport.clone(),
        command: first.command.clone(),
        args: first.args.clone(),
        env_vars: first.env_vars.clone(),
        env_http_headers: first.env_http_headers.clone(),
        bearer_token_env: first.bearer_token_env.clone(),
        url: first.url.clone(),
        package: first.package.clone(),
    };
    let mut rebuilt = old.clone();
    rebuilt.targets =
        targets_from_candidate(targets, &candidate, requirement.bearer_token_env.as_ref())?;
    Ok(rebuilt)
}

fn baselines(
    lock: &Lockfile,
    instructions: &[DiscoveredInstruction],
) -> Result<Vec<ProjectionBaseline>> {
    let mut output = Vec::new();
    for package in &lock.skill_packages {
        for skill in &package.skills {
            for target in &package.targets {
                output.push(ProjectionBaseline {
                    target: *target,
                    kind: "skill".into(),
                    key: skill.name.clone(),
                    sha256: skill.sha256.clone(),
                });
            }
        }
    }
    for server in &lock.mcp_servers {
        for mcp_target in &server.targets {
            output.push(ProjectionBaseline {
                target: mcp_target.target,
                kind: "mcp".into(),
                key: server.name.clone(),
                sha256: target::entry_digest(mcp_target)?,
            });
        }
    }
    output.extend(instruction::lock::baselines(instructions)?);
    output.sort();
    Ok(output)
}

#[derive(Serialize)]
struct ProjectionInput {
    lock_identity: String,
    targets: Vec<Target>,
    capability_schema: u32,
}

fn projection_input_hash(lock: &Lockfile, targets: &[Target]) -> Result<String> {
    let mut package_lock = lock.clone();
    package_lock.projection_baselines.clear();
    package_lock.projection_input_hash.clear();
    let mut targets = targets.to_vec();
    targets.sort();
    canonical_json_digest(&ProjectionInput {
        lock_identity: package_lock.lock_identity_digest()?,
        targets,
        capability_schema: if lock.version == 3 {
            7
        } else {
            ADAPTER_CAPABILITY_SCHEMA
        },
    })
}

fn validate_locked_mcp(
    lock: &Lockfile,
    expected: &BTreeMap<String, McpRequirement>,
    project_targets: &[Target],
) -> Result<()> {
    if lock.mcp_servers.len() != expected.len() {
        return Err(AruError::msg(
            "aru.lock does not contain the complete expected MCP set",
        ));
    }
    for (name, requirement) in expected {
        let server = lock
            .mcp_servers
            .iter()
            .find(|server| server.name == *name)
            .ok_or_else(|| AruError::msg(format!("aru.lock is missing MCP {name:?}")))?;
        let descriptor = canonical_json_digest(&normalized_mcp(requirement))?;
        if server.requirement != descriptor {
            return Err(AruError::msg(format!("aru.lock is stale for MCP {name:?}")));
        }
        let expected_targets =
            effective_mcp_targets(project_targets, requirement.targets.as_deref())
                .into_iter()
                .collect::<BTreeSet<_>>();
        let locked_targets = server
            .targets
            .iter()
            .map(|target| target.target)
            .collect::<BTreeSet<_>>();
        if expected_targets != locked_targets {
            return Err(AruError::msg(format!(
                "aru.lock lacks complete per-target projection selection for MCP {name:?}"
            )));
        }
    }
    Ok(())
}

fn validate_locked_projection(
    lock: &Lockfile,
    manifest: &Manifest,
    instructions: &[DiscoveredInstruction],
) -> Result<()> {
    for (name, requirement) in &manifest.mcp {
        let server = lock
            .mcp_servers
            .iter()
            .find(|server| server.name == *name)
            .ok_or_else(|| AruError::msg("aru.lock is missing an MCP server"))?;
        let expected: BTreeSet<_> =
            effective_mcp_targets(&manifest.project.targets, requirement.targets.as_deref())
                .into_iter()
                .collect();
        let locked_targets: BTreeSet<_> =
            server.targets.iter().map(|target| target.target).collect();
        if locked_targets != expected {
            return Err(AruError::msg(format!(
                "aru.lock lacks complete per-target projection selection for MCP {:?}",
                server.name
            )));
        }
    }
    let expected_baselines = baselines(lock, instructions)?;
    if lock.projection_baselines != expected_baselines {
        return Err(AruError::msg("aru.lock projection baseline is stale"));
    }
    let expected = projection_input_hash(lock, &manifest.project.targets)?;
    if lock.projection_input_hash != expected {
        return Err(AruError::msg(
            "aru.lock is stale for target projection inputs; run aru sync",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
