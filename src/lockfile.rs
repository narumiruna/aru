use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::digest::canonical_json_digest;
use crate::error::{AruError, IoContext, Result};
use crate::instruction::InstructionScope;
use crate::manifest::Target;

pub const LOCK_FILE: &str = "aru.lock";
pub const ADAPTER_CAPABILITY_SCHEMA: u32 = 9;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct ResourceOrigin {
    pub kind: String,
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct LockedSkill {
    pub name: String,
    pub path: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ResourceOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SkillPackage {
    pub source: String,
    pub requirement: String,
    pub version: String,
    pub revision: String,
    pub repository_name: String,
    pub targets: Vec<Target>,
    pub skills: Vec<LockedSkill>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct AruPackage {
    pub source: String,
    pub requirement: String,
    pub version: String,
    pub revision: String,
    pub name: String,
    pub package_version: String,
    pub manifest_sha256: String,
    pub content_sha256: String,
    pub targets: Vec<Target>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    #[serde(
        default,
        rename = "instruction-source",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub instruction_sources: Vec<LockedInstructionSource>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<LockedSkill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct LockedMcpPackage {
    pub registry: String,
    pub identifier: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct McpTarget {
    pub target: Target,
    pub kind: String,
    pub transport: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env_http_headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer_token_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<LockedMcpPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct McpServer {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ResourceOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    pub server_id: String,
    pub requirement: String,
    pub version: String,
    pub metadata_sha256: String,
    pub targets: Vec<McpTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct LockedInstructionSource {
    pub source: String,
    pub scope: InstructionScope,
    pub targets: Vec<Target>,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PluginManifestRecord {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PluginSelection {
    pub whole: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<crate::manifest::PluginComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct PluginPackage {
    pub name: String,
    pub source: String,
    pub requirement: String,
    pub version: String,
    pub revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub format: crate::manifest::PluginFormat,
    pub adapter_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    pub tree_sha256: String,
    pub manifests: Vec<PluginManifestRecord>,
    pub selection: PluginSelection,
    pub targets: Vec<Target>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<LockedSkill>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectionBaseline {
    pub target: Target,
    pub kind: String,
    pub key: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Lockfile {
    pub version: u32,
    pub package_input_hash: String,
    pub projection_input_hash: String,
    #[serde(
        default,
        rename = "instruction-source",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub instruction_sources: Vec<LockedInstructionSource>,
    #[serde(default, rename = "aru-package", skip_serializing_if = "Vec::is_empty")]
    pub aru_packages: Vec<AruPackage>,
    #[serde(
        default,
        rename = "skill-package",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub skill_packages: Vec<SkillPackage>,
    #[serde(default, rename = "mcp-server", skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServer>,
    #[serde(
        default,
        rename = "plugin-package",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub plugin_packages: Vec<PluginPackage>,
    #[serde(
        default,
        rename = "projection-baseline",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub projection_baselines: Vec<ProjectionBaseline>,
}

impl Lockfile {
    pub fn empty() -> Self {
        Self {
            version: 4,
            package_input_hash: String::new(),
            projection_input_hash: String::new(),
            instruction_sources: Vec::new(),
            aru_packages: Vec::new(),
            skill_packages: Vec::new(),
            mcp_servers: Vec::new(),
            plugin_packages: Vec::new(),
            projection_baselines: Vec::new(),
        }
    }

    pub fn load_optional(project: &Path) -> Result<Option<Self>> {
        let path = project.join(LOCK_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let text = std::fs::read_to_string(&path).at(&path)?;
        let lock: Self = toml::from_str(&text).map_err(|source| AruError::Toml {
            path: path.clone(),
            source,
        })?;
        lock.validate()?;
        Ok(Some(lock))
    }

    pub fn validate(&self) -> Result<()> {
        if !matches!(self.version, 3 | 4) {
            return Err(AruError::msg(format!(
                "unsupported aru.lock version {}; expected 3 or 4",
                self.version
            )));
        }
        if self.version == 3 && !self.plugin_packages.is_empty() {
            return Err(AruError::msg(
                "aru.lock v3 cannot contain plugin-package records",
            ));
        }
        let mut instruction_sources = BTreeSet::new();
        for instruction in &self.instruction_sources {
            if instruction.targets.is_empty() {
                return Err(AruError::msg(
                    "aru.lock contains instruction source without targets",
                ));
            }
            if !instruction_sources.insert(&instruction.source) {
                return Err(AruError::msg(
                    "aru.lock contains duplicate instruction source",
                ));
            }
            let unique_targets: BTreeSet<_> = instruction.targets.iter().collect();
            if unique_targets.len() != instruction.targets.len() {
                return Err(AruError::msg(
                    "aru.lock contains duplicate instruction source target",
                ));
            }
            if instruction
                .targets
                .iter()
                .any(|target| crate::target::capabilities(*target).instructions.is_none())
            {
                return Err(AruError::msg(
                    "aru.lock contains unsupported instruction source target",
                ));
            }
        }
        let mut package_sources = BTreeSet::new();
        let mut package_names = BTreeSet::new();
        let mut package_edges = 0_usize;
        for package in &self.aru_packages {
            if !package_sources.insert(package.source.as_str()) {
                return Err(AruError::msg(
                    "aru.lock contains duplicate aru package source",
                ));
            }
            if !package_names.insert(package.name.as_str()) {
                return Err(AruError::msg(
                    "aru.lock contains duplicate aru package name",
                ));
            }
            validate_revision(&package.revision, "aru package")?;
            let targets: BTreeSet<_> = package.targets.iter().collect();
            if package.targets.is_empty() || targets.len() != package.targets.len() {
                return Err(AruError::msg(
                    "aru.lock contains empty or duplicate aru package targets",
                ));
            }
            package_edges = package_edges.saturating_add(package.dependencies.len());
            if package_edges > crate::package::MAX_GRAPH_EDGES {
                return Err(AruError::msg(format!(
                    "aru.lock exceeds {} package dependency edges",
                    crate::package::MAX_GRAPH_EDGES
                )));
            }
            if package
                .instruction_sources
                .iter()
                .any(|source| !source.managed)
            {
                return Err(AruError::msg(
                    "aru.lock contains unmanaged instruction inside an aru package",
                ));
            }
        }
        if self.aru_packages.len() > crate::package::MAX_GRAPH_NODES {
            return Err(AruError::msg(format!(
                "aru.lock exceeds {} aru package nodes",
                crate::package::MAX_GRAPH_NODES
            )));
        }
        for package in &self.aru_packages {
            for dependency in &package.dependencies {
                if dependency == &package.source || !package_sources.contains(dependency.as_str()) {
                    return Err(AruError::msg(format!(
                        "aru.lock contains invalid aru package dependency {dependency:?}"
                    )));
                }
            }
            for instruction in &package.instruction_sources {
                if !self.instruction_sources.contains(instruction) {
                    return Err(AruError::msg(
                        "aru.lock package instruction is missing from the complete instruction lock",
                    ));
                }
            }
        }
        validate_package_graph(&self.aru_packages)?;

        let mut sources = BTreeSet::new();
        let mut skill_names = BTreeSet::new();
        for package in &self.skill_packages {
            if !sources.insert(&package.source) {
                return Err(AruError::msg("aru.lock contains duplicate skill source"));
            }
            validate_revision(&package.revision, "skill package")?;
            let unique_targets: BTreeSet<_> = package.targets.iter().collect();
            if package.targets.is_empty() || unique_targets.len() != package.targets.len() {
                return Err(AruError::msg(
                    "aru.lock contains empty or duplicate skill package targets",
                ));
            }
            if package
                .targets
                .iter()
                .any(|target| !crate::target::capabilities(*target).skills)
            {
                return Err(AruError::msg(
                    "aru.lock contains unsupported skill package target",
                ));
            }
            for skill in &package.skills {
                if !skill_names.insert(&skill.name) {
                    return Err(AruError::msg(format!(
                        "aru.lock contains duplicate resolved skill name {:?}",
                        skill.name
                    )));
                }
            }
        }
        let mut mcp_names = BTreeSet::new();
        for server in &self.mcp_servers {
            if server.targets.is_empty() {
                return Err(AruError::msg(
                    "aru.lock contains MCP server without targets",
                ));
            }
            if !mcp_names.insert(&server.name) {
                return Err(AruError::msg("aru.lock contains duplicate MCP name"));
            }
            let mut targets = BTreeSet::new();
            for target in &server.targets {
                if !crate::target::capabilities(target.target).mcp {
                    return Err(AruError::msg(
                        "aru.lock contains unsupported MCP server target",
                    ));
                }
                if !targets.insert(target.target) {
                    return Err(AruError::msg(format!(
                        "aru.lock contains duplicate target for MCP {:?}",
                        server.name
                    )));
                }
            }
        }
        for package in &self.aru_packages {
            let locked_skills = self
                .skill_packages
                .iter()
                .find(|skills| skills.source == package.source);
            if package.skills.is_empty() {
                if locked_skills.is_some() {
                    return Err(AruError::msg(
                        "aru.lock has unexpected skill package for an aru package without skills",
                    ));
                }
            } else if !locked_skills.is_some_and(|skills| {
                skills.revision == package.revision
                    && skills.targets == package.targets
                    && skills.skills == package.skills
            }) {
                return Err(AruError::msg(
                    "aru.lock aru package skills do not match the complete skill lock",
                ));
            }
            for name in &package.mcp {
                if !self.mcp_servers.iter().any(|server| server.name == *name) {
                    return Err(AruError::msg(
                        "aru.lock aru package MCP is missing from the complete MCP lock",
                    ));
                }
            }
        }
        let mut plugin_names = BTreeSet::new();
        for plugin in &self.plugin_packages {
            if !plugin_names.insert(&plugin.name) {
                return Err(AruError::msg("aru.lock contains duplicate plugin name"));
            }
            validate_revision(&plugin.revision, "plugin package")?;
            if plugin.adapter_version != crate::plugin::ADAPTER_VERSION {
                return Err(AruError::msg(format!(
                    "aru.lock plugin {:?} uses unsupported adapter version {}",
                    plugin.name, plugin.adapter_version
                )));
            }
            let targets: BTreeSet<_> = plugin.targets.iter().collect();
            if plugin.targets.is_empty() || targets.len() != plugin.targets.len() {
                return Err(AruError::msg(
                    "aru.lock contains empty or duplicate plugin targets",
                ));
            }
            let selected = !plugin.selection.components.is_empty()
                || !plugin.selection.skills.is_empty()
                || !plugin.selection.mcp.is_empty();
            if plugin.selection.whole == selected {
                return Err(AruError::msg(
                    "aru.lock contains inconsistent plugin selection intent",
                ));
            }
            let manifests = plugin
                .manifests
                .iter()
                .map(|manifest| manifest.path.as_str())
                .collect::<BTreeSet<_>>();
            if plugin.manifests.is_empty() || manifests.len() != plugin.manifests.len() {
                return Err(AruError::msg(
                    "aru.lock contains empty or duplicate plugin manifests",
                ));
            }
            for skill in &plugin.skills {
                if !skill.origin.as_ref().is_some_and(|origin| {
                    origin.kind == "plugin"
                        && origin.name == plugin.name
                        && origin.source == plugin.source
                }) {
                    return Err(AruError::msg(
                        "aru.lock plugin skill has invalid origin identity",
                    ));
                }
                if !self.skill_packages.iter().any(|package| {
                    package.revision == plugin.revision
                        && package.skills.iter().any(|locked| locked == skill)
                }) {
                    return Err(AruError::msg(
                        "aru.lock plugin skill is missing from the complete skill lock",
                    ));
                }
            }
            for name in &plugin.mcp {
                if !self.mcp_servers.iter().any(|server| {
                    server.name == *name
                        && server.origin.as_ref().is_some_and(|origin| {
                            origin.kind == "plugin"
                                && origin.name == plugin.name
                                && origin.source == plugin.source
                        })
                }) {
                    return Err(AruError::msg(
                        "aru.lock plugin MCP is missing from the complete MCP lock",
                    ));
                }
            }
        }
        let mut baselines = BTreeSet::new();
        for baseline in &self.projection_baselines {
            if !baselines.insert((baseline.target, &baseline.kind, &baseline.key)) {
                return Err(AruError::msg(
                    "aru.lock contains duplicate projection baseline",
                ));
            }
            let capabilities = crate::target::capabilities(baseline.target);
            let supported = match baseline.kind.as_str() {
                "instruction" => capabilities.instructions.is_some(),
                "skill" => capabilities.skills,
                "mcp" => capabilities.mcp,
                _ => false,
            };
            if !supported {
                return Err(AruError::msg(
                    "aru.lock contains unsupported projection baseline target",
                ));
            }
        }
        Ok(())
    }

    pub fn normalize(&mut self) {
        self.instruction_sources
            .sort_by(|left, right| left.source.cmp(&right.source));
        for instruction in &mut self.instruction_sources {
            instruction.targets.sort();
            instruction.targets.dedup();
        }
        self.aru_packages
            .sort_by(|left, right| left.source.cmp(&right.source));
        for package in &mut self.aru_packages {
            package.targets.sort();
            package.targets.dedup();
            package.dependencies.sort();
            package.dependencies.dedup();
            package
                .instruction_sources
                .sort_by(|left, right| left.source.cmp(&right.source));
            package
                .skills
                .sort_by(|left, right| left.name.cmp(&right.name));
            package.mcp.sort();
            package.mcp.dedup();
        }
        self.skill_packages
            .sort_by(|left, right| left.source.cmp(&right.source));
        for package in &mut self.skill_packages {
            package.targets.sort();
            package.targets.dedup();
            package
                .skills
                .sort_by(|left, right| left.name.cmp(&right.name));
        }
        self.mcp_servers
            .sort_by(|left, right| left.name.cmp(&right.name));
        for server in &mut self.mcp_servers {
            server.targets.sort_by_key(|target| target.target);
        }
        self.plugin_packages
            .sort_by(|left, right| left.name.cmp(&right.name));
        for plugin in &mut self.plugin_packages {
            plugin
                .manifests
                .sort_by(|left, right| left.path.cmp(&right.path));
            plugin.targets.sort();
            plugin.targets.dedup();
            plugin
                .skills
                .sort_by(|left, right| left.name.cmp(&right.name));
            plugin.mcp.sort();
            plugin.mcp.dedup();
            plugin.unsupported.sort();
            plugin.unsupported.dedup();
            plugin.diagnostics.sort();
            plugin.diagnostics.dedup();
            plugin.selection.components.sort();
            plugin.selection.components.dedup();
            plugin.selection.skills.sort();
            plugin.selection.skills.dedup();
            plugin.selection.mcp.sort();
            plugin.selection.mcp.dedup();
        }
        self.projection_baselines.sort();
    }

    pub fn bytes(&self) -> Result<Vec<u8>> {
        let mut normalized = self.clone();
        normalized.normalize();
        normalized.validate()?;
        let body = toml::to_string_pretty(&normalized)
            .map_err(|error| AruError::msg(format!("could not serialize aru.lock: {error}")))?;
        Ok(format!("# This file is generated by aru.\n{body}").into_bytes())
    }

    pub fn target_digest(target: &McpTarget) -> Result<String> {
        canonical_json_digest(target)
    }

    pub fn package_identity_digest(&self) -> Result<String> {
        #[derive(Serialize)]
        struct Identity<'a> {
            packages: &'a [AruPackage],
            skills: &'a [SkillPackage],
            mcp: &'a [McpServer],
            plugins: &'a [PluginPackage],
        }
        canonical_json_digest(&Identity {
            packages: &self.aru_packages,
            skills: &self.skill_packages,
            mcp: &self.mcp_servers,
            plugins: &self.plugin_packages,
        })
    }

    pub fn lock_identity_digest(&self) -> Result<String> {
        #[derive(Serialize)]
        struct Identity<'a> {
            instructions: &'a [LockedInstructionSource],
            packages: &'a [AruPackage],
            skills: &'a [SkillPackage],
            mcp: &'a [McpServer],
            plugins: &'a [PluginPackage],
        }
        canonical_json_digest(&Identity {
            instructions: &self.instruction_sources,
            packages: &self.aru_packages,
            skills: &self.skill_packages,
            mcp: &self.mcp_servers,
            plugins: &self.plugin_packages,
        })
    }
}

fn validate_revision(revision: &str, kind: &str) -> Result<()> {
    if revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AruError::msg(format!(
            "aru.lock contains invalid {kind} Git revision"
        )))
    }
}

fn validate_package_graph(packages: &[AruPackage]) -> Result<()> {
    fn visit<'a>(
        source: &'a str,
        by_source: &BTreeMap<&'a str, &'a AruPackage>,
        visiting: &mut BTreeSet<&'a str>,
        complete: &mut BTreeSet<&'a str>,
        depth: usize,
    ) -> Result<()> {
        if depth > crate::package::MAX_GRAPH_DEPTH {
            return Err(AruError::msg(format!(
                "aru.lock package graph exceeds maximum depth {}",
                crate::package::MAX_GRAPH_DEPTH
            )));
        }
        if complete.contains(source) {
            return Ok(());
        }
        if !visiting.insert(source) {
            return Err(AruError::msg(format!(
                "aru.lock package graph contains a cycle at {source:?}"
            )));
        }
        let package = by_source
            .get(source)
            .ok_or_else(|| AruError::msg("aru.lock package graph references a missing node"))?;
        for dependency in &package.dependencies {
            visit(dependency, by_source, visiting, complete, depth + 1)?;
        }
        visiting.remove(source);
        complete.insert(source);
        Ok(())
    }

    let by_source = packages
        .iter()
        .map(|package| (package.source.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    let mut visiting = BTreeSet::new();
    let mut complete = BTreeSet::new();
    for source in by_source.keys() {
        visit(source, &by_source, &mut visiting, &mut complete, 1)?;
    }
    Ok(())
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialization_is_stable_after_reordering() {
        let mut left = Lockfile::empty();
        left.package_input_hash = "sha256:a".into();
        left.projection_input_hash = "sha256:b".into();
        left.projection_baselines = vec![
            ProjectionBaseline {
                target: Target::Claude,
                kind: "skill".into(),
                key: "z".into(),
                sha256: "sha256:z".into(),
            },
            ProjectionBaseline {
                target: Target::Codex,
                kind: "skill".into(),
                key: "a".into(),
                sha256: "sha256:a".into(),
            },
        ];
        let mut right = left.clone();
        right.projection_baselines.reverse();
        assert_eq!(left.bytes().unwrap(), right.bytes().unwrap());
    }

    #[test]
    fn instruction_sources_are_normalized_and_change_only_complete_lock_identity() {
        let mut lock = Lockfile::empty();
        let package_identity = lock.package_identity_digest().unwrap();
        let lock_identity = lock.lock_identity_digest().unwrap();
        lock.instruction_sources.push(LockedInstructionSource {
            source: "AGENTS.md".into(),
            scope: InstructionScope::SourceDirectory {
                directory: ".".into(),
            },
            targets: vec![Target::Copilot, Target::Claude],
            sha256: "sha256:source".into(),
            managed: false,
        });
        assert_eq!(package_identity, lock.package_identity_digest().unwrap());
        assert_ne!(lock_identity, lock.lock_identity_digest().unwrap());
        let bytes = lock.bytes().unwrap();
        let parsed: Lockfile = toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        assert_eq!(
            parsed.instruction_sources[0].targets,
            [Target::Claude, Target::Copilot]
        );
    }

    #[test]
    fn duplicate_instruction_sources_are_rejected() {
        let source = LockedInstructionSource {
            source: "AGENTS.md".into(),
            scope: InstructionScope::SourceDirectory {
                directory: ".".into(),
            },
            targets: vec![Target::Claude],
            sha256: "sha256:source".into(),
            managed: false,
        };
        let mut lock = Lockfile::empty();
        lock.instruction_sources = vec![source.clone(), source];
        assert!(lock.validate().is_err());
    }

    #[test]
    fn skill_only_targets_are_rejected_for_instruction_lock_records() {
        let mut lock = Lockfile::empty();
        lock.instruction_sources.push(LockedInstructionSource {
            source: "AGENTS.md".into(),
            scope: InstructionScope::SourceDirectory {
                directory: ".".into(),
            },
            targets: vec![Target::Kiro],
            sha256: "sha256:source".into(),
            managed: false,
        });
        assert!(
            lock.validate()
                .unwrap_err()
                .to_string()
                .contains("unsupported instruction source target")
        );

        let mut lock = Lockfile::empty();
        lock.projection_baselines.push(ProjectionBaseline {
            target: Target::Kiro,
            kind: "instruction".into(),
            key: "AGENTS.md".into(),
            sha256: "sha256:source".into(),
        });
        assert!(
            lock.validate()
                .unwrap_err()
                .to_string()
                .contains("unsupported projection baseline target")
        );
    }

    #[test]
    fn incomplete_target_and_duplicate_baseline_records_are_rejected() {
        let mut lock = Lockfile::empty();
        lock.mcp_servers.push(McpServer {
            name: "docs".into(),
            origin: None,
            registry: None,
            server_id: "docs".into(),
            requirement: "sha256:requirement".into(),
            version: "direct".into(),
            metadata_sha256: "sha256:metadata".into(),
            targets: Vec::new(),
        });
        assert!(
            lock.validate()
                .unwrap_err()
                .to_string()
                .contains("without targets")
        );

        let mut lock = Lockfile::empty();
        let baseline = ProjectionBaseline {
            target: Target::Codex,
            kind: "skill".into(),
            key: "demo".into(),
            sha256: "sha256:demo".into(),
        };
        lock.projection_baselines = vec![baseline.clone(), baseline];
        assert!(
            lock.validate()
                .unwrap_err()
                .to_string()
                .contains("duplicate projection baseline")
        );
    }

    #[test]
    fn package_graph_cycles_are_rejected() {
        let package = |source: &str, name: &str, dependency: &str| AruPackage {
            source: source.into(),
            requirement: "version:^1".into(),
            version: "1.0.0".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            name: name.into(),
            package_version: "1.0.0".into(),
            manifest_sha256: "sha256:manifest".into(),
            content_sha256: "sha256:content".into(),
            targets: vec![Target::Codex],
            dependencies: vec![dependency.into()],
            instruction_sources: Vec::new(),
            skills: Vec::new(),
            mcp: Vec::new(),
        };
        let mut lock = Lockfile::empty();
        lock.aru_packages = vec![
            package(
                "git+https://example.com/a.git",
                "a",
                "git+https://example.com/b.git",
            ),
            package(
                "git+https://example.com/b.git",
                "b",
                "git+https://example.com/a.git",
            ),
        ];
        assert!(lock.validate().unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn pypi_mcp_package_round_trips_with_exact_uvx_identity() {
        let target = McpTarget {
            target: Target::Codex,
            kind: "package".into(),
            transport: "stdio".into(),
            command: Some("uvx".into()),
            args: vec!["weather-mcp@0.5.0".into()],
            env_vars: vec!["WEATHER_API_KEY".into()],
            env_http_headers: BTreeMap::new(),
            url: None,
            bearer_token_env: None,
            package: Some(LockedMcpPackage {
                registry: "pypi".into(),
                identifier: "weather-mcp".into(),
                version: "0.5.0".into(),
            }),
        };
        let mut lock = Lockfile::empty();
        lock.mcp_servers.push(McpServer {
            name: "weather".into(),
            origin: None,
            registry: Some(crate::registry::DEFAULT_REGISTRY.into()),
            server_id: "io.example/weather".into(),
            requirement: "sha256:requirement".into(),
            version: "0.5.0".into(),
            metadata_sha256: "sha256:metadata".into(),
            targets: vec![target],
        });

        let bytes = lock.bytes().unwrap();
        let replayed: Lockfile = toml::from_str(std::str::from_utf8(&bytes).unwrap()).unwrap();
        replayed.validate().unwrap();
        assert_eq!(replayed.mcp_servers, lock.mcp_servers);
        assert_eq!(replayed.bytes().unwrap(), bytes);
    }

    #[test]
    fn v3_and_v4_golden_locks_round_trip_to_identical_bytes() {
        for fixture in [
            include_str!("../tests/fixtures/contracts/aru-v3.lock"),
            include_str!("../tests/fixtures/contracts/aru.lock"),
        ] {
            let lock: Lockfile = toml::from_str(fixture).unwrap();
            lock.validate().unwrap();
            assert_eq!(lock.bytes().unwrap(), fixture.as_bytes());
        }
    }
}
