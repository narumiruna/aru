use std::path::Path;

use serde_json::{Map, Value};

use crate::error::Result;
use crate::lockfile::McpTarget;
use crate::target::json_mcp;

pub const CONFIG_PATH: &str = ".mcp.json";

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    root: Map<String, Value>,
}

impl ClaudeConfig {
    pub fn load(project: &Path) -> Result<Self> {
        Ok(Self {
            root: json_mcp::load(project, CONFIG_PATH)?,
        })
    }

    pub fn digest(&self, name: &str) -> Result<Option<String>> {
        json_mcp::digest(&self.root, name)
    }

    pub fn set(&mut self, name: &str, target: &McpTarget) -> Result<()> {
        json_mcp::set(&mut self.root, CONFIG_PATH, name, target)
    }

    pub fn remove(&mut self, name: &str) {
        json_mcp::remove(&mut self.root, name);
    }

    pub fn bytes(&self) -> Result<Vec<u8>> {
        json_mcp::bytes(&self.root, CONFIG_PATH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Target;

    #[test]
    fn merge_preserves_unrelated_keys() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join(CONFIG_PATH),
            r#"{"custom":{"keep":true},"mcpServers":{"unmanaged":{"command":"keep"}}}"#,
        )
        .unwrap();
        let mut config = ClaudeConfig::load(project.path()).unwrap();
        config
            .set(
                "managed",
                &McpTarget {
                    target: Target::Claude,
                    kind: "package".into(),
                    transport: "stdio".into(),
                    command: Some("npx".into()),
                    args: vec!["--yes".into(), "pkg@1".into()],
                    env_vars: Vec::new(),
                    env_http_headers: std::collections::BTreeMap::new(),
                    url: None,
                    bearer_token_env: None,
                    package: None,
                },
            )
            .unwrap();
        let value: Value = serde_json::from_slice(&config.bytes().unwrap()).unwrap();
        assert_eq!(value["custom"]["keep"], true);
        assert_eq!(value["mcpServers"]["unmanaged"]["command"], "keep");
    }
}
