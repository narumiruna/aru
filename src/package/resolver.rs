use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cache::Cache;
use crate::digest::{canonical_json_digest, sha256_bytes};
use crate::error::{AruError, IoContext, Result};
use crate::instruction::DiscoveredInstruction;
use crate::lockfile::{AruPackage, LockedSkill, Lockfile, SkillPackage};
use crate::manifest::{
    MANIFEST_FILE, Manifest, McpRequirement, PackageRequirement, PackageTrust, Project,
    SkillRequirement, Target,
};
use crate::skill::{DiscoveredSkill, discover_and_select};
use crate::source::git::{self, GitSource};

use super::{
    MAX_GRAPH_DEPTH, MAX_GRAPH_EDGES, MAX_GRAPH_NODES, PackageManifest, TreeBudget, tree_digest,
    validate_tree,
};

pub struct ResolveOptions<'a> {
    pub previous: Option<&'a Lockfile>,
    pub locked: bool,
    pub offline: bool,
    pub update: &'a BTreeSet<String>,
    pub precise: &'a BTreeMap<String, String>,
}

pub struct PackageInspection {
    pub source: String,
    pub version: String,
    pub revision: String,
    pub manifest: PackageManifest,
}

pub struct PackageResolution {
    pub packages: Vec<AruPackage>,
    pub instructions: Vec<DiscoveredInstruction>,
    pub skill_packages: Vec<SkillPackage>,
    pub skill_sources: BTreeMap<String, PathBuf>,
    pub mcp: BTreeMap<String, McpRequirement>,
}

struct Node {
    source: GitSource,
    requirement: String,
    version: String,
    revision: String,
    manifest_sha256: String,
    content_sha256: String,
    manifest: PackageManifest,
    checkout: PathBuf,
    targets: BTreeSet<Target>,
    dependencies: BTreeSet<String>,
}

struct GraphResolver<'a> {
    project: &'a Path,
    cache: &'a Cache,
    options: ResolveOptions<'a>,
    nodes: BTreeMap<String, Node>,
    visiting: BTreeSet<String>,
    edge_count: usize,
    budget: TreeBudget,
    validate_only: bool,
}

pub fn inspect_source(
    project: &Path,
    raw_source: &str,
    offline: bool,
) -> Result<PackageInspection> {
    let source = git::canonicalize(project, raw_source)?;
    if offline && !source.is_local() {
        return Err(AruError::msg(format!(
            "offline mode cannot inspect undeclared remote aru package {}",
            source.identity
        )));
    }
    let resolved = git::resolve(&source, Some("*"), None, None)?;
    let cache = Cache::ephemeral_for_project(project)?;
    let checkout = cache.checkout_with_policy(&source, &resolved.revision, offline)?;
    let mut budget = TreeBudget::default();
    let (manifest, _, _) = load_checkout(&checkout, &mut budget)?;
    if semver::Version::parse(&resolved.version).is_ok()
        && manifest.package.version != resolved.version
    {
        return Err(AruError::msg(format!(
            "aru package {} declares version {}, but Git tag resolved {}",
            manifest.package.name, manifest.package.version, resolved.version
        )));
    }
    Ok(PackageInspection {
        source: source.identity,
        version: resolved.version,
        revision: resolved.revision,
        manifest,
    })
}

pub fn resolve(
    project: &Path,
    manifest: &Manifest,
    cache: &Cache,
    options: ResolveOptions<'_>,
) -> Result<PackageResolution> {
    let trust = canonical_trust(project, &manifest.package_trust)?;
    let mut resolver = GraphResolver {
        project,
        cache,
        options,
        nodes: BTreeMap::new(),
        visiting: BTreeSet::new(),
        edge_count: 0,
        budget: TreeBudget::default(),
        validate_only: false,
    };
    let mut root_identities = BTreeMap::<String, String>::new();
    for (declared_source, requirement) in &manifest.packages {
        let source = git::canonicalize(project, declared_source)?;
        if let Some(previous) =
            root_identities.insert(source.identity.clone(), declared_source.clone())
        {
            return Err(AruError::msg(format!(
                "aru package sources {previous:?} and {declared_source:?} identify the same repository"
            )));
        }
        resolver.resolve_node(
            source,
            requirement.clone(),
            &manifest.project.targets,
            None,
            1,
            true,
        )?;
    }
    for identity in trust.keys() {
        if !resolver.nodes.contains_key(identity) {
            return Err(AruError::msg(format!(
                "package trust source {identity:?} is not present in the resolved package graph"
            )));
        }
    }
    resolver.finish(trust)
}

pub fn validate_archive_graph(
    root: &Path,
    manifest: &PackageManifest,
    offline: bool,
) -> Result<()> {
    let source = git::canonicalize(root, ".")?;
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .map_err(|error| AruError::msg(format!("could not inspect package snapshot: {error}")))?;
    if !output.status.success() {
        return Err(AruError::msg(format!(
            "could not inspect package snapshot: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let revision = String::from_utf8(output.stdout)
        .map_err(|_| AruError::msg("package snapshot revision is not UTF-8"))?
        .trim()
        .to_owned();
    let targets = if manifest.skills.is_empty() && manifest.mcp.is_empty() {
        vec![
            Target::Agents,
            Target::Codex,
            Target::Claude,
            Target::Copilot,
            Target::Opencode,
            Target::Pi,
        ]
    } else {
        vec![Target::Codex, Target::Claude]
    };
    let requirement = PackageRequirement {
        rev: Some(revision),
        targets: Some(targets.clone()),
        ..PackageRequirement::default()
    };
    let cache = Cache::ephemeral()?;
    let updates = BTreeSet::new();
    let precise = BTreeMap::new();
    let mut resolver = GraphResolver {
        project: root,
        cache: &cache,
        options: ResolveOptions {
            previous: None,
            locked: false,
            offline,
            update: &updates,
            precise: &precise,
        },
        nodes: BTreeMap::new(),
        visiting: BTreeSet::new(),
        edge_count: 0,
        budget: TreeBudget::default(),
        validate_only: true,
    };
    resolver.resolve_node(source, requirement, &targets, None, 1, true)?;
    resolver.finish(BTreeMap::new())?;
    Ok(())
}

impl GraphResolver<'_> {
    #[allow(clippy::too_many_arguments)]
    fn resolve_node(
        &mut self,
        source: GitSource,
        mut requirement: PackageRequirement,
        parent_targets: &[Target],
        parent: Option<&str>,
        depth: usize,
        root: bool,
    ) -> Result<()> {
        if depth > MAX_GRAPH_DEPTH {
            return Err(AruError::msg(format!(
                "aru package graph exceeds maximum depth {MAX_GRAPH_DEPTH}"
            )));
        }
        requirement.normalize();
        requirement.validate(&source.identity, parent_targets)?;
        let targets = requirement
            .targets
            .clone()
            .unwrap_or_else(|| parent_targets.to_vec())
            .into_iter()
            .collect::<BTreeSet<_>>();
        let descriptor = requirement_descriptor(&requirement);
        let identity = source.identity.clone();

        if let Some(parent) = parent {
            let inserted = self
                .nodes
                .get_mut(parent)
                .expect("parent package exists while resolving dependencies")
                .dependencies
                .insert(identity.clone());
            if inserted {
                self.edge_count += 1;
                if self.edge_count > MAX_GRAPH_EDGES {
                    return Err(AruError::msg(format!(
                        "aru package graph exceeds {MAX_GRAPH_EDGES} package dependency edges"
                    )));
                }
            }
        }
        if self.visiting.contains(&identity) {
            return Err(AruError::msg(format!(
                "aru package dependency cycle reaches {identity:?}"
            )));
        }

        let needs_expansion;
        if let Some(existing) = self.nodes.get_mut(&identity) {
            if existing.requirement != descriptor {
                return Err(AruError::msg(format!(
                    "aru package {identity:?} is requested with conflicting requirements {:?} and {descriptor:?}",
                    existing.requirement
                )));
            }
            let previous_len = existing.targets.len();
            existing.targets.extend(targets);
            needs_expansion = existing.targets.len() != previous_len;
        } else {
            if self.nodes.len() >= MAX_GRAPH_NODES {
                return Err(AruError::msg(format!(
                    "aru package graph exceeds {MAX_GRAPH_NODES} package nodes"
                )));
            }
            if !root && source.is_local() {
                return Err(AruError::msg(format!(
                    "transitive local package {identity:?} is not reproducible; use a remote Git source"
                )));
            }
            let old = self.options.previous.and_then(|lock| {
                lock.aru_packages
                    .iter()
                    .find(|package| package.source == identity)
            });
            let (version, revision) = self.resolve_reference(&source, &requirement, old)?;
            let (checkout, manifest, manifest_sha256, content_sha256) =
                self.checkout_manifest(&source, &revision, old)?;
            if semver::Version::parse(&version).is_ok() && manifest.package.version != version {
                return Err(AruError::msg(format!(
                    "aru package {} declares version {}, but Git tag resolved {}",
                    manifest.package.name, manifest.package.version, version
                )));
            }
            if self.edge_count.saturating_add(manifest.dependencies.len()) > MAX_GRAPH_EDGES {
                return Err(AruError::msg(format!(
                    "aru package graph exceeds {MAX_GRAPH_EDGES} package dependency edges"
                )));
            }
            self.nodes.insert(
                identity.clone(),
                Node {
                    source,
                    requirement: descriptor,
                    version,
                    revision,
                    manifest_sha256,
                    content_sha256,
                    manifest,
                    checkout,
                    targets,
                    dependencies: BTreeSet::new(),
                },
            );
            needs_expansion = true;
        }

        if !needs_expansion {
            return Ok(());
        }
        self.visiting.insert(identity.clone());
        let (dependencies, inherited_targets) = {
            let node = self.nodes.get(&identity).unwrap();
            (
                node.manifest.dependencies.clone(),
                node.targets.iter().copied().collect::<Vec<_>>(),
            )
        };
        for (declared_source, dependency) in dependencies {
            let dependency_source = git::canonicalize(self.project, &declared_source)?;
            self.resolve_node(
                dependency_source,
                dependency,
                &inherited_targets,
                Some(&identity),
                depth + 1,
                false,
            )?;
        }
        self.visiting.remove(&identity);
        Ok(())
    }

    fn resolve_reference(
        &self,
        source: &GitSource,
        requirement: &PackageRequirement,
        old: Option<&AruPackage>,
    ) -> Result<(String, String)> {
        let old = old.map(|package| git::LockedReference {
            requirement: &package.requirement,
            version: &package.version,
            revision: &package.revision,
        });
        let resolved = git::select_reference(
            source,
            package_reference(requirement),
            old,
            git::ReferencePolicy {
                locked: self.options.locked,
                update: self.options.update.contains(&source.identity),
                offline: self.options.offline,
                precise: self
                    .options
                    .precise
                    .get(&source.identity)
                    .map(String::as_str),
                fallback_branch: None,
            },
            &format!("aru package {}", source.identity),
        )?;
        Ok((resolved.version, resolved.revision))
    }

    fn checkout_manifest(
        &mut self,
        source: &GitSource,
        revision: &str,
        old: Option<&AruPackage>,
    ) -> Result<(PathBuf, PackageManifest, String, String)> {
        let mut checkout =
            self.cache
                .checkout_with_policy(source, revision, self.options.offline)?;
        let mut loaded = load_checkout(&checkout, &mut self.budget);
        if let (Ok((_, manifest_digest, content_digest)), Some(old)) = (&loaded, old)
            && old.revision == revision
            && (old.manifest_sha256 != *manifest_digest || old.content_sha256 != *content_digest)
        {
            self.cache.invalidate(source, revision)?;
            checkout = self
                .cache
                .checkout_with_policy(source, revision, self.options.offline)?;
            loaded = load_checkout(&checkout, &mut self.budget);
        }
        let (manifest, manifest_digest, content_digest) = loaded?;
        if let Some(old) = old
            && old.revision == revision
            && (old.manifest_sha256 != manifest_digest || old.content_sha256 != content_digest)
        {
            return Err(AruError::msg(format!(
                "aru package manifest for locked revision {revision} does not match aru.lock"
            )));
        }
        Ok((checkout, manifest, manifest_digest, content_digest))
    }

    fn finish(self, trust: BTreeMap<String, PackageTrust>) -> Result<PackageResolution> {
        let mut packages = Vec::new();
        let mut instructions = Vec::new();
        let mut skill_packages = Vec::new();
        let mut skill_sources = BTreeMap::new();
        let mut mcp = BTreeMap::new();
        let mut package_names = BTreeSet::new();
        let mut skill_names = BTreeSet::new();
        let mut mcp_names = BTreeSet::new();

        for (identity, node) in self.nodes {
            if !package_names.insert(node.manifest.package.name.clone()) {
                return Err(AruError::msg(format!(
                    "resolved aru package name {:?} is provided by more than one source",
                    node.manifest.package.name
                )));
            }
            let targets = node.targets.iter().copied().collect::<Vec<_>>();
            let package_instructions = package_instructions(&identity, &node, &targets)?;
            let locked_instructions =
                crate::instruction::lock::locked_sources(&package_instructions);
            instructions.extend(package_instructions);

            let selected_skills = if node.manifest.skills.is_empty() {
                Vec::new()
            } else {
                if targets
                    .iter()
                    .any(|target| !crate::target::capabilities(*target).skills)
                {
                    return Err(AruError::msg(format!(
                        "aru package {:?} exports skills unsupported by one or more effective targets; narrow its targets",
                        node.manifest.package.name
                    )));
                }
                package_skills(&node)?
            };
            for skill in &selected_skills {
                if !skill_names.insert(skill.name.clone()) {
                    return Err(AruError::msg(format!(
                        "resolved skill name {:?} is provided by more than one aru package",
                        skill.name
                    )));
                }
                skill_sources.insert(skill.name.clone(), skill.absolute_path.clone());
            }
            if !selected_skills.is_empty() {
                skill_packages.push(SkillPackage {
                    source: node.source.identity.clone(),
                    requirement: format!("aru-package:{}", node.requirement),
                    version: node.manifest.package.version.clone(),
                    revision: node.revision.clone(),
                    repository_name: node.manifest.package.name.clone(),
                    targets: targets.clone(),
                    skills: selected_skills.iter().map(locked_skill).collect(),
                });
            }

            let allowed = trust.get(&identity);
            if let Some(allowed) = allowed {
                for name in &allowed.mcp {
                    if !node.manifest.mcp.contains_key(name) {
                        return Err(AruError::msg(format!(
                            "package trust for {identity:?} names unknown MCP {name:?}"
                        )));
                    }
                }
            }
            for (name, requirement) in &node.manifest.mcp {
                if !self.validate_only && !allowed.is_some_and(|trust| trust.mcp.contains(name)) {
                    return Err(AruError::msg(format!(
                        "untrusted package MCP {name:?} from {identity}; add an explicit package-trust decision"
                    )));
                }
                if !mcp_names.insert(name.clone()) {
                    return Err(AruError::msg(format!(
                        "resolved MCP name {name:?} is provided by more than one aru package"
                    )));
                }
                let mut requirement = requirement.clone();
                requirement.validate_targets(name, &targets)?;
                let effective = requirement
                    .targets
                    .clone()
                    .unwrap_or_else(|| targets.clone());
                if effective
                    .iter()
                    .any(|target| !crate::target::capabilities(*target).mcp)
                {
                    return Err(AruError::msg(format!(
                        "package MCP {name:?} is unsupported by one or more effective targets; narrow its targets"
                    )));
                }
                requirement.targets = Some(effective);
                mcp.insert(name.clone(), requirement);
            }

            packages.push(AruPackage {
                source: node.source.identity,
                requirement: node.requirement,
                version: node.version,
                revision: node.revision,
                name: node.manifest.package.name,
                package_version: node.manifest.package.version,
                manifest_sha256: node.manifest_sha256,
                content_sha256: node.content_sha256,
                targets,
                dependencies: node.dependencies.into_iter().collect(),
                instruction_sources: locked_instructions,
                skills: selected_skills.iter().map(locked_skill).collect(),
                mcp: node.manifest.mcp.keys().cloned().collect(),
            });
        }
        packages.sort_by(|left, right| left.source.cmp(&right.source));
        skill_packages.sort_by(|left, right| left.source.cmp(&right.source));
        instructions.sort_by(|left, right| left.unit.source.cmp(&right.unit.source));

        if self.options.locked {
            let expected = self
                .options
                .previous
                .map(|lock| lock.aru_packages.as_slice())
                .unwrap_or_default();
            if packages != expected {
                return Err(AruError::msg(
                    "aru.lock is stale for the resolved aru package graph",
                ));
            }
        }
        Ok(PackageResolution {
            packages,
            instructions,
            skill_packages,
            skill_sources,
            mcp,
        })
    }
}

fn package_instructions(
    identity: &str,
    node: &Node,
    targets: &[Target],
) -> Result<Vec<DiscoveredInstruction>> {
    let synthetic = Manifest {
        project: Project {
            targets: targets.to_vec(),
        },
        instructions: node.manifest.instructions.clone(),
        skills: BTreeMap::new(),
        mcp: BTreeMap::new(),
        packages: BTreeMap::new(),
        package_trust: BTreeMap::new(),
    };
    synthetic.validate()?;
    let mut discovered = crate::instruction::discovery::discover(&node.checkout, &synthetic)?;
    let namespace = canonical_json_digest(&identity)?
        .trim_start_matches("sha256:")
        .to_owned();
    for instruction in &mut discovered {
        let relative = instruction.unit.source.to_string_lossy().replace('\\', "/");
        instruction.unit.source = PathBuf::from(format!("packages/{namespace}/{relative}"));
        instruction.unit.managed = true;
    }
    Ok(discovered)
}

fn package_skills(node: &Node) -> Result<Vec<DiscoveredSkill>> {
    let mut requirement = SkillRequirement {
        include: node.manifest.skills.keys().cloned().collect(),
        paths: node.manifest.skills.clone(),
        ..SkillRequirement::default()
    };
    requirement.normalize();
    discover_and_select(&node.checkout, &node.manifest.package.name, &requirement)
}

fn locked_skill(skill: &DiscoveredSkill) -> LockedSkill {
    LockedSkill {
        name: skill.name.clone(),
        path: skill.relative_path.clone(),
        sha256: skill.sha256.clone(),
    }
}

fn canonical_trust(
    project: &Path,
    declared: &BTreeMap<String, PackageTrust>,
) -> Result<BTreeMap<String, PackageTrust>> {
    let mut trust = BTreeMap::new();
    for (source, value) in declared {
        let canonical = git::canonicalize(project, source)?;
        let mut value = value.clone();
        value.normalize();
        if trust.insert(canonical.identity.clone(), value).is_some() {
            return Err(AruError::msg(format!(
                "multiple package trust entries identify {}",
                canonical.identity
            )));
        }
    }
    Ok(trust)
}

fn package_reference(requirement: &PackageRequirement) -> git::ReferenceSpec<'_> {
    git::ReferenceSpec::new(
        requirement.version.as_deref(),
        requirement.branch.as_deref(),
        requirement.rev.as_deref(),
    )
}

fn requirement_descriptor(requirement: &PackageRequirement) -> String {
    package_reference(requirement).descriptor()
}

fn load_checkout(
    checkout: &Path,
    budget: &mut TreeBudget,
) -> Result<(PackageManifest, String, String)> {
    validate_tree(checkout, budget)?;
    let manifest = PackageManifest::load(checkout)?;
    let path = checkout.join(MANIFEST_FILE);
    let manifest_digest = sha256_bytes(&std::fs::read(&path).at(&path)?);
    let content_digest = tree_digest(checkout)?;
    Ok((manifest, manifest_digest, content_digest))
}
