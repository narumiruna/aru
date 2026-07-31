use std::path::Path;

use crate::error::{AruError, Result};
use crate::lockfile::McpTarget;
use crate::manifest::Target;
use crate::target::{claude::ClaudeConfig, codex::CodexConfig};

#[derive(Debug, Clone)]
pub(crate) enum McpConfig {
    Codex(CodexConfig),
    Claude(ClaudeConfig),
}

impl McpConfig {
    pub(crate) fn load(project: &Path, target: Target) -> Result<Self> {
        match target {
            Target::Codex => CodexConfig::load(project).map(Self::Codex),
            Target::Claude => ClaudeConfig::load(project).map(Self::Claude),
            Target::Copilot | Target::Pi | Target::Opencode => Err(AruError::msg(format!(
                "internal error: MCP projection reached unsupported target {target}"
            ))),
        }
    }

    pub(crate) fn digest(&self, name: &str) -> Result<Option<String>> {
        match self {
            Self::Codex(config) => config.digest(name),
            Self::Claude(config) => config.digest(name),
        }
    }

    pub(crate) fn set(&mut self, name: &str, target: &McpTarget) -> Result<()> {
        match self {
            Self::Codex(config) if target.target == Target::Codex => config.set(name, target),
            Self::Claude(config) if target.target == Target::Claude => config.set(name, target),
            _ => Err(AruError::msg(
                "internal error: MCP target does not match its configuration adapter",
            )),
        }
    }

    pub(crate) fn remove(&mut self, name: &str) {
        match self {
            Self::Codex(config) => config.remove(name),
            Self::Claude(config) => config.remove(name),
        }
    }

    pub(crate) fn bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::Codex(config) => Ok(config.bytes()),
            Self::Claude(config) => config.bytes(),
        }
    }
}

pub(crate) fn destination(target: Target) -> Option<&'static str> {
    match target {
        Target::Codex => Some(crate::target::codex::CONFIG_PATH),
        Target::Claude => Some(crate::target::claude::CONFIG_PATH),
        Target::Copilot | Target::Pi | Target::Opencode => None,
    }
}

pub(crate) fn target_for_destination(destination: &str) -> Option<Target> {
    match destination {
        crate::target::codex::CONFIG_PATH => Some(Target::Codex),
        crate::target::claude::CONFIG_PATH => Some(Target::Claude),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_round_trip_to_supported_targets() {
        for target in [Target::Codex, Target::Claude] {
            assert_eq!(
                target_for_destination(destination(target).unwrap()),
                Some(target)
            );
        }
        assert_eq!(destination(Target::Copilot), None);
    }
}
