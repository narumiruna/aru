use std::path::Path;

use crate::error::{AruError, Result};
use crate::lockfile::McpTarget;
use crate::manifest::Target;
use crate::target::{
    claude::ClaudeConfig, codex::CodexConfig, copilot::CopilotConfig, opencode::OpencodeConfig,
};

#[derive(Debug, Clone)]
pub(crate) enum McpConfig {
    Codex(CodexConfig),
    Claude(ClaudeConfig),
    Copilot(CopilotConfig),
    Opencode(OpencodeConfig),
}

impl McpConfig {
    pub(crate) fn load(project: &Path, target: Target) -> Result<Self> {
        match target {
            Target::Codex => CodexConfig::load(project).map(Self::Codex),
            Target::Claude => ClaudeConfig::load(project).map(Self::Claude),
            Target::Copilot => CopilotConfig::load(project).map(Self::Copilot),
            Target::Opencode => OpencodeConfig::load(project).map(Self::Opencode),
            Target::Pi => Err(AruError::msg(format!(
                "internal error: MCP projection reached unsupported target {target}"
            ))),
        }
    }

    pub(crate) fn digest(&self, name: &str) -> Result<Option<String>> {
        match self {
            Self::Codex(config) => config.digest(name),
            Self::Claude(config) => config.digest(name),
            Self::Copilot(config) => config.digest(name),
            Self::Opencode(config) => config.digest(name),
        }
    }

    pub(crate) fn set(&mut self, name: &str, target: &McpTarget) -> Result<()> {
        match self {
            Self::Codex(config) if target.target == Target::Codex => config.set(name, target),
            Self::Claude(config) if target.target == Target::Claude => config.set(name, target),
            Self::Copilot(config) if target.target == Target::Copilot => config.set(name, target),
            Self::Opencode(config) if target.target == Target::Opencode => config.set(name, target),
            _ => Err(AruError::msg(
                "internal error: MCP target does not match its configuration adapter",
            )),
        }
    }

    pub(crate) fn remove(&mut self, name: &str) {
        match self {
            Self::Codex(config) => config.remove(name),
            Self::Claude(config) => config.remove(name),
            Self::Copilot(config) => config.remove(name),
            Self::Opencode(config) => config.remove(name),
        }
    }

    pub(crate) fn bytes(&self) -> Result<Vec<u8>> {
        match self {
            Self::Codex(config) => Ok(config.bytes()),
            Self::Claude(config) => config.bytes(),
            Self::Copilot(config) => config.bytes(),
            Self::Opencode(config) => config.bytes(),
        }
    }
}

pub(crate) fn destination(target: Target) -> Option<&'static str> {
    match target {
        Target::Codex => Some(crate::target::codex::CONFIG_PATH),
        Target::Claude => Some(crate::target::claude::CONFIG_PATH),
        Target::Copilot => Some(crate::target::copilot::CONFIG_PATH),
        Target::Opencode => Some(crate::target::opencode::CONFIG_PATH),
        Target::Pi => None,
    }
}

pub(crate) fn target_for_destination(destination: &str) -> Option<Target> {
    match destination {
        crate::target::codex::CONFIG_PATH => Some(Target::Codex),
        crate::target::claude::CONFIG_PATH => Some(Target::Claude),
        crate::target::copilot::CONFIG_PATH => Some(Target::Copilot),
        crate::target::opencode::CONFIG_PATH => Some(Target::Opencode),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destinations_round_trip_to_supported_targets() {
        for target in [
            Target::Codex,
            Target::Claude,
            Target::Copilot,
            Target::Opencode,
        ] {
            assert_eq!(
                target_for_destination(destination(target).unwrap()),
                Some(target)
            );
        }
        assert_eq!(destination(Target::Pi), None);
    }
}
