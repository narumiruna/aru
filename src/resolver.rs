use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cache::Cache;
use crate::digest::canonical_json_digest;
use crate::error::{AruError, Result};
use crate::instruction::{self, DiscoveredInstruction};
use crate::lockfile::{
    ADAPTER_CAPABILITY_SCHEMA, LockedSkill, Lockfile, McpServer, McpTarget, ProjectionBaseline,
    SkillPackage,
};
use crate::manifest::{Manifest, McpRequirement, SkillRequirement, Target};
use crate::registry::{RegistryClient, ResolvedCandidate};
use crate::skill::{DiscoveredSkill, discover_and_select, discover_candidates};
use crate::source::git::{self, GitSource};
use crate::target;

#[derive(Debug)]
pub struct Resolution {
    pub lock: Lockfile,
    pub skill_sources: BTreeMap<String, PathBuf>,
    pub instructions: Vec<DiscoveredInstruction>,
}

pub struct ResolveOptions<'a> {
    pub previous: Option<&'a Lockfile>,
    pub locked: bool,
    pub update_skills: &'a BTreeSet<String>,
    pub update_mcp: &'a BTreeSet<String>,
    pub dry_run: bool,
    pub skill_hints: &'a BTreeMap<String, SkillResolutionHint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillResolutionHint {
    pub requirement: String,
    pub version: String,
    pub revision: String,
}

#[derive(Debug, Clone)]
pub struct SkillSourceInspection {
    pub source: String,
    pub requirement: String,
    pub version: String,
    pub revision: String,
    pub candidates: Vec<DiscoveredSkill>,
}

impl SkillSourceInspection {
    pub fn hint(&self) -> SkillResolutionHint {
        SkillResolutionHint {
            requirement: self.requirement.clone(),
            version: self.version.clone(),
            revision: self.revision.clone(),
        }
    }
}

pub fn resolve(
    project: &Path,
    manifest: &Manifest,
    options: ResolveOptions<'_>,
) -> Result<Resolution> {
    let sources = canonical_sources(project, manifest)?;
    let instructions = instruction::discovery::discover(project, manifest)?;
    let skill_targets = target::skill_targets(&manifest.project.targets);
    let mcp_targets = target::mcp_targets(&manifest.project.targets);
    if !manifest.skills.is_empty() && skill_targets.is_empty() {
        return Err(AruError::msg(
            "no configured target supports Agent Skills projections",
        ));
    }
    if !manifest.mcp.is_empty() && mcp_targets.is_empty() {
        return Err(AruError::msg(
            "no configured target supports MCP projections",
        ));
    }
    let package_input_hash = package_input_hash(manifest, &sources)?;
    let cache = if options.dry_run {
        Cache::ephemeral()?
    } else {
        Cache::project(project)
    };

    if options.locked {
        let lock = options
            .previous
            .cloned()
            .ok_or_else(|| AruError::msg("aru sync --locked requires an existing aru.lock"))?;
        if lock.package_input_hash != package_input_hash {
            return Err(AruError::msg(
                "aru.lock is stale for package inputs; run aru lock or aru sync",
            ));
        }
        instruction::lock::validate_locked_sources(&lock.instruction_sources, &instructions)?;
        validate_locked_projection(&lock, manifest, &instructions)?;
        let skill_sources = materialize_locked(&cache, manifest, &sources, &lock)?;
        return Ok(Resolution {
            lock,
            skill_sources,
            instructions,
        });
    }

    let previous = options.previous;
    let mut skill_packages = Vec::new();
    let mut skill_sources = BTreeMap::new();
    let mut resolved_names = BTreeSet::new();
    for (manifest_source, requirement) in &manifest.skills {
        let source = sources.get(manifest_source).unwrap();
        let descriptor = skill_requirement_descriptor(requirement);
        let old = previous.and_then(|lock| {
            lock.skill_packages
                .iter()
                .find(|package| package.source == source.identity)
        });
        let update = options.update_skills.contains(&source.identity);
        let (version, revision) = if let Some(hint) = options.skill_hints.get(&source.identity) {
            validate_skill_hint(hint, &source.identity, requirement)?;
            (hint.version.clone(), hint.revision.clone())
        } else {
            resolve_skill_reference(source, requirement, old, update)?
        };
        let checkout = cache.checkout(source, &revision)?;
        let mut selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
        if let Some(old) = old.filter(|_| !update)
            && selected_digest_mismatch(&selected, old)
        {
            cache.invalidate(source, &revision)?;
            let checkout = cache.checkout(source, &revision)?;
            selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
            if selected_digest_mismatch(&selected, old) {
                return Err(AruError::msg(format!(
                    "content for locked Git revision {} does not match aru.lock",
                    revision
                )));
            }
        }
        for skill in &selected {
            if !resolved_names.insert(skill.name.clone()) {
                return Err(AruError::msg(format!(
                    "resolved skill name {:?} is provided by more than one package",
                    skill.name
                )));
            }
            skill_sources.insert(skill.name.clone(), skill.absolute_path.clone());
        }
        skill_packages.push(SkillPackage {
            source: source.identity.clone(),
            requirement: descriptor,
            version,
            revision,
            repository_name: source.repository_name.clone(),
            skills: selected.iter().map(locked_skill).collect(),
        });
    }

    let registry_client = RegistryClient::new()?;
    let mut mcp_servers = Vec::new();
    for (name, requirement) in &manifest.mcp {
        let descriptor = canonical_json_digest(&normalized_mcp(requirement))?;
        let old =
            previous.and_then(|lock| lock.mcp_servers.iter().find(|server| server.name == *name));
        let reusable = old.filter(|server| {
            !options.update_mcp.contains(name) && server.requirement == descriptor
        });
        let server = if let Some(old) = reusable {
            rebuild_mcp_targets(old, requirement, &mcp_targets)?
        } else {
            resolve_mcp(
                &registry_client,
                name,
                requirement,
                &mcp_targets,
                &descriptor,
            )?
        };
        mcp_servers.push(server);
    }

    let mut lock = Lockfile {
        version: 1,
        package_input_hash,
        projection_input_hash: String::new(),
        instruction_sources: instruction::lock::locked_sources(&instructions),
        skill_packages,
        mcp_servers,
        projection_baselines: Vec::new(),
    };
    lock.normalize();
    lock.projection_baselines = baselines(&lock, &manifest.project.targets, &instructions)?;
    lock.projection_input_hash = projection_input_hash(&lock, &manifest.project.targets)?;
    lock.normalize();
    lock.validate()?;
    Ok(Resolution {
        lock,
        skill_sources,
        instructions,
    })
}

pub fn inspect_skill_source(
    project: &Path,
    manifest_source: &str,
    requirement: &SkillRequirement,
    previous: Option<&Lockfile>,
    dry_run: bool,
) -> Result<SkillSourceInspection> {
    let source = git::canonicalize(project, manifest_source)?;
    let descriptor = skill_requirement_descriptor(requirement);
    let old = previous.and_then(|lock| {
        lock.skill_packages
            .iter()
            .find(|package| package.source == source.identity)
    });
    let (version, revision) = resolve_skill_reference(&source, requirement, old, false)?;
    let cache = if dry_run {
        Cache::ephemeral()?
    } else {
        Cache::project(project)
    };
    let mut checkout = cache.checkout(&source, &revision)?;
    let mut candidates =
        discover_candidates(&checkout, &source.repository_name, &requirement.paths)?;
    if let Some(old) = old
        && selected_digest_mismatch(&candidates, old)
    {
        cache.invalidate(&source, &revision)?;
        checkout = cache.checkout(&source, &revision)?;
        candidates = discover_candidates(&checkout, &source.repository_name, &requirement.paths)?;
        if selected_digest_mismatch(&candidates, old) {
            return Err(AruError::msg(format!(
                "content for locked Git revision {} does not match aru.lock",
                revision
            )));
        }
    }
    Ok(SkillSourceInspection {
        source: source.identity,
        requirement: descriptor,
        version,
        revision,
        candidates,
    })
}

fn resolve_skill_reference(
    source: &GitSource,
    requirement: &SkillRequirement,
    old: Option<&SkillPackage>,
    update: bool,
) -> Result<(String, String)> {
    let descriptor = skill_requirement_descriptor(requirement);
    if !update
        && old.is_some_and(|package| {
            package.requirement == descriptor
                && requirement
                    .rev
                    .as_ref()
                    .is_none_or(|rev| package.revision.starts_with(&rev.to_ascii_lowercase()))
                && requirement
                    .branch
                    .as_ref()
                    .is_none_or(|branch| package.version == *branch)
                && requirement.version.as_deref().is_none_or(|_| {
                    git::locked_version_matches(requirement.version.as_deref(), &package.version)
                })
        })
    {
        let package = old.unwrap();
        Ok((package.version.clone(), package.revision.clone()))
    } else {
        let resolved = git::resolve(
            source,
            requirement.version.as_deref(),
            requirement.branch.as_deref(),
            requirement.rev.as_deref(),
        )?;
        Ok((resolved.version, resolved.revision))
    }
}

fn validate_skill_hint(
    hint: &SkillResolutionHint,
    source_identity: &str,
    requirement: &SkillRequirement,
) -> Result<()> {
    if hint.requirement != skill_requirement_descriptor(requirement) {
        return Err(AruError::msg(format!(
            "interactive skill preview for {source_identity:?} no longer matches the requirement"
        )));
    }
    let revision_valid =
        hint.revision.len() == 40 && hint.revision.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !revision_valid
        || requirement
            .rev
            .as_ref()
            .is_some_and(|revision| !hint.revision.starts_with(&revision.to_ascii_lowercase()))
        || requirement
            .branch
            .as_ref()
            .is_some_and(|branch| hint.version != *branch)
        || requirement.version.as_deref().is_some_and(|_| {
            !git::locked_version_matches(requirement.version.as_deref(), &hint.version)
        })
    {
        return Err(AruError::msg(format!(
            "interactive skill preview for {source_identity:?} has an invalid resolved revision"
        )));
    }
    Ok(())
}

fn canonical_sources(project: &Path, manifest: &Manifest) -> Result<BTreeMap<String, GitSource>> {
    let mut output = BTreeMap::new();
    let mut identities = BTreeMap::<String, String>::new();
    for source in manifest.skills.keys() {
        let canonical = git::canonicalize(project, source)?;
        if let Some(previous) = identities.insert(canonical.identity.clone(), source.clone()) {
            return Err(AruError::msg(format!(
                "skill sources {previous:?} and {source:?} identify the same repository"
            )));
        }
        output.insert(source.clone(), canonical);
    }
    Ok(output)
}

#[derive(Serialize)]
struct PackageInputs {
    skills: Vec<SkillInput>,
    mcp: BTreeMap<String, McpRequirement>,
}

#[derive(Serialize)]
struct SkillInput {
    source: String,
    requirement: SkillRequirement,
}

fn package_input_hash(
    manifest: &Manifest,
    sources: &BTreeMap<String, GitSource>,
) -> Result<String> {
    let mut skills: Vec<_> = manifest
        .skills
        .iter()
        .map(|(key, requirement)| {
            let mut requirement = requirement.clone();
            requirement.normalize();
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
    canonical_json_digest(&PackageInputs { skills, mcp })
}

fn normalized_mcp(requirement: &McpRequirement) -> McpRequirement {
    let mut normalized = requirement.clone();
    if normalized.server.is_some() {
        if normalized.registry.is_none() {
            normalized.registry = Some(crate::registry::DEFAULT_REGISTRY.into());
        }
        normalized.version = Some(normalize_semver_requirement(
            normalized.version.as_deref().unwrap_or("*"),
        ));
    } else if normalized.url.is_some() && normalized.transport.is_none() {
        normalized.transport = Some("streamable-http".into());
    }
    normalized
}

fn normalize_semver_requirement(requirement: &str) -> String {
    semver::VersionReq::parse(requirement)
        .map(|requirement| requirement.to_string())
        .unwrap_or_else(|_| requirement.to_owned())
}

fn skill_requirement_descriptor(requirement: &SkillRequirement) -> String {
    if let Some(rev) = &requirement.rev {
        format!("rev:{}", rev.to_ascii_lowercase())
    } else if let Some(branch) = &requirement.branch {
        format!("branch:{branch}")
    } else {
        format!(
            "version:{}",
            normalize_semver_requirement(requirement.version.as_deref().unwrap_or("*"))
        )
    }
}

fn locked_skill(skill: &DiscoveredSkill) -> LockedSkill {
    LockedSkill {
        name: skill.name.clone(),
        path: skill.relative_path.clone(),
        sha256: skill.sha256.clone(),
    }
}

fn selected_digest_mismatch(selected: &[DiscoveredSkill], old: &SkillPackage) -> bool {
    selected.iter().any(|skill| {
        old.skills
            .iter()
            .find(|locked| locked.name == skill.name && locked.path == skill.relative_path)
            .is_some_and(|locked| locked.sha256 != skill.sha256)
    })
}

fn materialize_locked(
    cache: &Cache,
    manifest: &Manifest,
    sources: &BTreeMap<String, GitSource>,
    lock: &Lockfile,
) -> Result<BTreeMap<String, PathBuf>> {
    let mut output = BTreeMap::new();
    for (manifest_source, requirement) in &manifest.skills {
        let source = sources.get(manifest_source).unwrap();
        let package = lock
            .skill_packages
            .iter()
            .find(|package| package.source == source.identity)
            .ok_or_else(|| AruError::msg("aru.lock is missing a skill package"))?;
        let mut checkout = cache.checkout(source, &package.revision)?;
        let mut selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
        if selected_matches_lock(&selected, package).is_err() {
            cache.invalidate(source, &package.revision)?;
            checkout = cache.checkout(source, &package.revision)?;
            selected = discover_and_select(&checkout, &source.repository_name, requirement)?;
            selected_matches_lock(&selected, package)?;
        }
        for skill in selected {
            if output
                .insert(skill.name.clone(), skill.absolute_path)
                .is_some()
            {
                return Err(AruError::msg("aru.lock resolves duplicate skill names"));
            }
        }
    }
    Ok(output)
}

fn selected_matches_lock(selected: &[DiscoveredSkill], package: &SkillPackage) -> Result<()> {
    let found: Vec<_> = selected.iter().map(locked_skill).collect();
    if found == package.skills {
        Ok(())
    } else {
        Err(AruError::msg(format!(
            "materialized skill content for {} does not match aru.lock",
            package.source
        )))
    }
}

fn resolve_mcp(
    client: &RegistryClient,
    name: &str,
    requirement: &McpRequirement,
    targets: &[Target],
    descriptor: &str,
) -> Result<McpServer> {
    let (server_id, registry, version, metadata_sha256, candidate) =
        if let Some(url) = &requirement.url {
            let candidate = ResolvedCandidate {
                kind: "remote".into(),
                transport: requirement
                    .transport
                    .clone()
                    .unwrap_or_else(|| "streamable-http".into()),
                command: None,
                args: Vec::new(),
                env_vars: Vec::new(),
                env_http_headers: BTreeMap::new(),
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
    targets: &[Target],
    instructions: &[DiscoveredInstruction],
) -> Result<Vec<ProjectionBaseline>> {
    let mut output = Vec::new();
    let skill_targets = target::skill_targets(targets);
    for package in &lock.skill_packages {
        for skill in &package.skills {
            for target in &skill_targets {
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
        capability_schema: ADAPTER_CAPABILITY_SCHEMA,
    })
}

fn validate_locked_projection(
    lock: &Lockfile,
    manifest: &Manifest,
    instructions: &[DiscoveredInstruction],
) -> Result<()> {
    let configured_targets: BTreeSet<_> = target::mcp_targets(&manifest.project.targets)
        .into_iter()
        .collect();
    for server in &lock.mcp_servers {
        let locked_targets: BTreeSet<_> =
            server.targets.iter().map(|target| target.target).collect();
        if locked_targets != configured_targets {
            return Err(AruError::msg(format!(
                "aru.lock lacks complete per-target projection selection for MCP {:?}",
                server.name
            )));
        }
    }
    let expected_baselines = baselines(lock, &manifest.project.targets, instructions)?;
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

pub fn canonical_update_skill_targets(
    project: &Path,
    manifest: &Manifest,
    requested: &[String],
) -> Result<BTreeSet<String>> {
    if requested.is_empty() {
        return manifest
            .skills
            .keys()
            .map(|source| git::canonicalize(project, source).map(|source| source.identity))
            .collect();
    }
    let sources = canonical_sources(project, manifest)?;
    let mut output = BTreeSet::new();
    for request in requested {
        let canonical = git::canonicalize(project, request)?;
        if !sources
            .values()
            .any(|source| source.identity == canonical.identity)
        {
            return Err(AruError::msg(format!(
                "skill source {request:?} is not declared in aru.toml"
            )));
        }
        output.insert(canonical.identity);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git(repository: &Path, arguments: &[&str]) -> String {
        let output = Command::new("git")
            .current_dir(repository)
            .args(arguments)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {arguments:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn write_skill(repository: &Path, name: &str) {
        let directory = repository.join("skills").join(name);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: Test\n---\n# Test\n"),
        )
        .unwrap();
    }

    #[test]
    fn inspection_reuses_locks_and_hint_pins_the_previewed_revision() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        let project = temporary.path().join("project");
        std::fs::create_dir(&repository).unwrap();
        std::fs::create_dir(&project).unwrap();
        git(&repository, &["init", "--quiet"]);
        git(&repository, &["config", "user.email", "test@example.com"]);
        git(&repository, &["config", "user.name", "Test"]);
        write_skill(&repository, "zeta");
        write_skill(&repository, "alpha");
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "--quiet", "-m", "first"]);
        git(&repository, &["tag", "1.0.0"]);
        let first_revision = git(&repository, &["rev-parse", "HEAD"]);

        let requirement = SkillRequirement {
            version: Some("=1.0.0".into()),
            ..SkillRequirement::default()
        };
        let manifest = Manifest {
            project: crate::manifest::Project {
                targets: vec![Target::Codex],
            },
            instructions: crate::manifest::Instructions::default(),
            skills: BTreeMap::from([(
                repository.to_string_lossy().into_owned(),
                requirement.clone(),
            )]),
            mcp: BTreeMap::new(),
        };
        let empty_hints = BTreeMap::new();
        let initial = resolve(
            &project,
            &manifest,
            ResolveOptions {
                previous: None,
                locked: false,
                update_skills: &BTreeSet::new(),
                update_mcp: &BTreeSet::new(),
                dry_run: false,
                skill_hints: &empty_hints,
            },
        )
        .unwrap();

        std::fs::write(repository.join("changed"), "second").unwrap();
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "--quiet", "-m", "second"]);
        let second_revision = git(&repository, &["rev-parse", "HEAD"]);
        git(&repository, &["tag", "--force", "1.0.0"]);

        let inspection = inspect_skill_source(
            &project,
            &repository.to_string_lossy(),
            &requirement,
            Some(&initial.lock),
            false,
        )
        .unwrap();
        assert_eq!(inspection.revision, first_revision);
        assert_eq!(
            inspection
                .candidates
                .iter()
                .map(|candidate| candidate.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );

        let hints = BTreeMap::from([(inspection.source.clone(), inspection.hint())]);
        let pinned = resolve(
            &project,
            &manifest,
            ResolveOptions {
                previous: None,
                locked: false,
                update_skills: &BTreeSet::new(),
                update_mcp: &BTreeSet::new(),
                dry_run: false,
                skill_hints: &hints,
            },
        )
        .unwrap();
        assert_eq!(pinned.lock.skill_packages[0].revision, first_revision);

        let changed_requirement = SkillRequirement {
            version: None,
            rev: Some(second_revision.clone()),
            ..requirement.clone()
        };
        let changed = inspect_skill_source(
            &project,
            &repository.to_string_lossy(),
            &changed_requirement,
            Some(&initial.lock),
            false,
        )
        .unwrap();
        assert_eq!(changed.revision, second_revision);

        let invalid = SkillResolutionHint {
            requirement: "version:>=9.0.0".into(),
            ..inspection.hint()
        };
        assert!(validate_skill_hint(&invalid, &inspection.source, &requirement).is_err());
    }

    #[test]
    fn branch_inspection_reuses_lock_update_moves_and_hint_pins_head() {
        let temporary = tempfile::tempdir().unwrap();
        let repository = temporary.path().join("repository");
        let project = temporary.path().join("project");
        std::fs::create_dir(&repository).unwrap();
        std::fs::create_dir(&project).unwrap();
        git(&repository, &["init", "--quiet"]);
        git(&repository, &["config", "user.email", "test@example.com"]);
        git(&repository, &["config", "user.name", "Test"]);
        write_skill(&repository, "alpha");
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "--quiet", "-m", "first"]);
        git(&repository, &["branch", "live"]);
        let first_revision = git(&repository, &["rev-parse", "HEAD"]);

        let requirement = SkillRequirement {
            branch: Some("live".into()),
            ..SkillRequirement::default()
        };
        let manifest = Manifest {
            project: crate::manifest::Project {
                targets: vec![Target::Codex],
            },
            instructions: crate::manifest::Instructions::default(),
            skills: BTreeMap::from([(
                repository.to_string_lossy().into_owned(),
                requirement.clone(),
            )]),
            mcp: BTreeMap::new(),
        };
        let empty_hints = BTreeMap::new();
        let first = resolve(
            &project,
            &manifest,
            ResolveOptions {
                previous: None,
                locked: false,
                update_skills: &BTreeSet::new(),
                update_mcp: &BTreeSet::new(),
                dry_run: false,
                skill_hints: &empty_hints,
            },
        )
        .unwrap();
        assert_eq!(first.lock.skill_packages[0].requirement, "branch:live");
        assert_eq!(first.lock.skill_packages[0].version, "live");

        std::fs::write(repository.join("changed"), "second").unwrap();
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "--quiet", "-m", "second"]);
        git(&repository, &["branch", "--force", "live", "HEAD"]);
        let second_revision = git(&repository, &["rev-parse", "HEAD"]);

        let inspection = inspect_skill_source(
            &project,
            &repository.to_string_lossy(),
            &requirement,
            Some(&first.lock),
            false,
        )
        .unwrap();
        assert_eq!(inspection.revision, first_revision);
        let hints = BTreeMap::from([(inspection.source.clone(), inspection.hint())]);
        let pinned = resolve(
            &project,
            &manifest,
            ResolveOptions {
                previous: None,
                locked: false,
                update_skills: &BTreeSet::new(),
                update_mcp: &BTreeSet::new(),
                dry_run: false,
                skill_hints: &hints,
            },
        )
        .unwrap();
        assert_eq!(pinned.lock.skill_packages[0].revision, first_revision);

        let source = first.lock.skill_packages[0].source.clone();
        let updated = resolve(
            &project,
            &manifest,
            ResolveOptions {
                previous: Some(&first.lock),
                locked: false,
                update_skills: &BTreeSet::from([source]),
                update_mcp: &BTreeSet::new(),
                dry_run: false,
                skill_hints: &empty_hints,
            },
        )
        .unwrap();
        assert_eq!(updated.lock.skill_packages[0].revision, second_revision);
    }

    #[test]
    fn package_hash_excludes_targets() {
        let source = GitSource {
            identity: "git+https://example.com/a.git".into(),
            fetch: "https://example.com/a.git".into(),
            repository_name: "a".into(),
        };
        let mut sources = BTreeMap::new();
        sources.insert("source".into(), source);
        let mut manifest = Manifest {
            project: crate::manifest::Project {
                targets: vec![Target::Codex],
            },
            instructions: crate::manifest::Instructions::default(),
            skills: BTreeMap::from([("source".into(), SkillRequirement::default())]),
            mcp: BTreeMap::new(),
        };
        let first = package_input_hash(&manifest, &sources).unwrap();
        manifest.project.targets.push(Target::Claude);
        assert_eq!(first, package_input_hash(&manifest, &sources).unwrap());

        manifest.skills.get_mut("source").unwrap().include = vec!["zeta".into(), "alpha".into()];
        let selectors = package_input_hash(&manifest, &sources).unwrap();
        manifest.skills.get_mut("source").unwrap().include.reverse();
        assert_eq!(selectors, package_input_hash(&manifest, &sources).unwrap());
    }

    #[test]
    fn projection_hash_sorts_target_set() {
        let lock = Lockfile::empty();
        let forward = projection_input_hash(&lock, &[Target::Codex, Target::Claude]).unwrap();
        let reverse = projection_input_hash(&lock, &[Target::Claude, Target::Codex]).unwrap();
        assert_eq!(forward, reverse);
    }
}
