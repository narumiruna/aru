use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::digest::canonical_json_digest;
use crate::error::{AruError, IoContext, Result};
use crate::manifest::Agent;

pub const LOCK_FILE: &str = "aru.lock";
pub const ADAPTER_CAPABILITY_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct LockedSkill {
    pub name: String,
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct SkillPackage {
    pub source: String,
    pub requirement: String,
    pub version: String,
    pub revision: String,
    pub repository_name: String,
    pub skills: Vec<LockedSkill>,
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
    pub agent: Agent,
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
    pub registry: Option<String>,
    pub server_id: String,
    pub requirement: String,
    pub version: String,
    pub metadata_sha256: String,
    pub targets: Vec<McpTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub struct ProjectionBaseline {
    pub agent: Agent,
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
        rename = "skill-package",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub skill_packages: Vec<SkillPackage>,
    #[serde(default, rename = "mcp-server", skip_serializing_if = "Vec::is_empty")]
    pub mcp_servers: Vec<McpServer>,
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
            version: 1,
            package_input_hash: String::new(),
            projection_input_hash: String::new(),
            skill_packages: Vec::new(),
            mcp_servers: Vec::new(),
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
        if self.version != 1 {
            return Err(AruError::msg(format!(
                "unsupported aru.lock version {}; expected 1",
                self.version
            )));
        }
        let mut sources = BTreeSet::new();
        let mut skill_names = BTreeSet::new();
        for package in &self.skill_packages {
            if !sources.insert(&package.source) {
                return Err(AruError::msg("aru.lock contains duplicate skill source"));
            }
            if package.revision.len() != 40
                || !package
                    .revision
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(AruError::msg("aru.lock contains invalid Git revision"));
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
            if !mcp_names.insert(&server.name) {
                return Err(AruError::msg("aru.lock contains duplicate MCP name"));
            }
            let mut agents = BTreeSet::new();
            for target in &server.targets {
                if !agents.insert(target.agent) {
                    return Err(AruError::msg(format!(
                        "aru.lock contains duplicate target for MCP {:?}",
                        server.name
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn normalize(&mut self) {
        self.skill_packages
            .sort_by(|left, right| left.source.cmp(&right.source));
        for package in &mut self.skill_packages {
            package
                .skills
                .sort_by(|left, right| left.name.cmp(&right.name));
        }
        self.mcp_servers
            .sort_by(|left, right| left.name.cmp(&right.name));
        for server in &mut self.mcp_servers {
            server.targets.sort_by_key(|target| target.agent);
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
            skills: &'a [SkillPackage],
            mcp: &'a [McpServer],
        }
        canonical_json_digest(&Identity {
            skills: &self.skill_packages,
            mcp: &self.mcp_servers,
        })
    }
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
                agent: Agent::ClaudeCode,
                kind: "skill".into(),
                key: "z".into(),
                sha256: "sha256:z".into(),
            },
            ProjectionBaseline {
                agent: Agent::Codex,
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
    fn v1_golden_lock_round_trips_to_identical_bytes() {
        let fixture = include_str!("../tests/fixtures/contracts/aru.lock");
        let lock: Lockfile = toml::from_str(fixture).unwrap();
        lock.validate().unwrap();
        assert_eq!(lock.bytes().unwrap(), fixture.as_bytes());
    }
}
