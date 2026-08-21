use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    AruError, Result, Target, sort_dedup, validate_branch_name, validate_dependency_targets,
    validate_name,
};

pub fn validate_plugin_name(name: &str) -> Result<()> {
    let valid = (1..=64).contains(&name.len())
        && !name.starts_with(['-', '.'])
        && !name.ends_with(['-', '.'])
        && !name.contains("--")
        && !name.contains("..")
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
        });
    if valid {
        Ok(())
    } else {
        Err(AruError::msg(format!("invalid plugin name {name:?}")))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum PluginFormat {
    AgentPlugins,
    Openai,
    Gemini,
}

impl std::fmt::Display for PluginFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentPlugins => write!(f, "agent-plugins"),
            Self::Openai => write!(f, "openai"),
            Self::Gemini => write!(f, "gemini"),
        }
    }
}

impl std::str::FromStr for PluginFormat {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "agent-plugins" => Ok(Self::AgentPlugins),
            "openai" => Ok(Self::Openai),
            "gemini" => Ok(Self::Gemini),
            _ => Err(format!(
                "unknown plugin format {value:?}; expected agent-plugins, openai, or gemini"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum PluginComponent {
    Skills,
    Mcp,
}

impl std::fmt::Display for PluginComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Skills => write!(f, "skills"),
            Self::Mcp => write!(f, "mcp"),
        }
    }
}

impl std::str::FromStr for PluginComponent {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "skills" => Ok(Self::Skills),
            "mcp" => Ok(Self::Mcp),
            _ => Err(format!(
                "unknown plugin component {value:?}; expected skills or mcp"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRequirement {
    pub source: String,
    pub format: PluginFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<PluginComponent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<Target>>,
}

impl PluginRequirement {
    pub fn whole_plugin(&self) -> bool {
        self.components.is_empty() && self.skills.is_empty() && self.mcp.is_empty()
    }

    pub fn normalize(&mut self) {
        self.components.sort();
        self.components.dedup();
        sort_dedup(&mut self.skills);
        sort_dedup(&mut self.mcp);
        if let Some(targets) = &mut self.targets {
            targets.sort();
            targets.dedup();
        }
    }

    pub fn validate(&self, name: &str, project_targets: &[Target]) -> Result<()> {
        validate_plugin_name(name)?;
        if self.source.is_empty() {
            return Err(AruError::msg(format!(
                "plugin {name:?} source must not be empty"
            )));
        }
        let references = usize::from(self.version.is_some())
            + usize::from(self.branch.is_some())
            + usize::from(self.rev.is_some());
        if references > 1 {
            return Err(AruError::msg(format!(
                "plugin {name:?} can set only one of version, branch, or rev"
            )));
        }
        if let Some(version) = &self.version {
            semver::VersionReq::parse(version).map_err(|error| {
                AruError::msg(format!(
                    "invalid plugin SemVer requirement {version:?}: {error}"
                ))
            })?;
        }
        if let Some(branch) = &self.branch {
            validate_branch_name(branch)?;
        }
        if let Some(revision) = &self.rev {
            let valid = (7..=40).contains(&revision.len())
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit());
            if !valid {
                return Err(AruError::msg(format!(
                    "invalid plugin Git revision {revision:?}"
                )));
            }
        }
        if let Some(subdir) = &self.subdir {
            crate::skill::validate_relative_selector(subdir)?;
        }
        if self.components.iter().collect::<BTreeSet<_>>().len() != self.components.len()
            || self.skills.iter().collect::<BTreeSet<_>>().len() != self.skills.len()
            || self.mcp.iter().collect::<BTreeSet<_>>().len() != self.mcp.len()
        {
            return Err(AruError::msg(format!(
                "plugin {name:?} selection contains duplicates"
            )));
        }
        if self.components.contains(&PluginComponent::Skills) && !self.skills.is_empty() {
            return Err(AruError::msg(format!(
                "plugin {name:?} cannot combine components = [\"skills\"] with named skills"
            )));
        }
        if self.components.contains(&PluginComponent::Mcp) && !self.mcp.is_empty() {
            return Err(AruError::msg(format!(
                "plugin {name:?} cannot combine components = [\"mcp\"] with named MCP servers"
            )));
        }
        for skill in &self.skills {
            validate_name(skill, "plugin skill name")?;
        }
        for mcp in &self.mcp {
            validate_name(mcp, "plugin MCP name")?;
        }
        validate_dependency_targets(
            self.targets.as_deref(),
            project_targets,
            &format!("plugin {name:?}"),
            |_| true,
            "configured targets",
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginTrust {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
}

impl PluginTrust {
    pub fn normalize(&mut self) {
        sort_dedup(&mut self.mcp);
    }

    pub fn validate(&self, name: &str) -> Result<()> {
        if self.mcp.is_empty() {
            return Err(AruError::msg(format!(
                "plugin trust {name:?} must name at least one MCP server"
            )));
        }
        if self.mcp.iter().collect::<BTreeSet<_>>().len() != self.mcp.len() {
            return Err(AruError::msg(format!(
                "plugin trust {name:?} contains duplicate MCP names"
            )));
        }
        for item in &self.mcp {
            validate_name(item, "trusted plugin MCP name")?;
        }
        Ok(())
    }
}
