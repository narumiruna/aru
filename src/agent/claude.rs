use std::path::Path;

use serde_json::{Map, Value};

use crate::agent::normalized_entry;
use crate::digest::canonical_json_digest;
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::McpTarget;

pub const CONFIG_PATH: &str = ".mcp.json";

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    root: Map<String, Value>,
}

impl ClaudeConfig {
    pub fn load(project: &Path) -> Result<Self> {
        let path = project.join(CONFIG_PATH);
        let root = if path.exists() {
            let bytes = std::fs::read(&path).at(&path)?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|source| AruError::Json {
                path: path.clone(),
                source,
            })?;
            value
                .as_object()
                .cloned()
                .ok_or_else(|| AruError::msg(".mcp.json root must be a JSON object"))?
        } else {
            Map::new()
        };
        if root
            .get("mcpServers")
            .is_some_and(|value| !value.is_object())
        {
            return Err(AruError::msg(".mcp.json mcpServers must be an object"));
        }
        Ok(Self { root })
    }

    pub fn digest(&self, name: &str) -> Result<Option<String>> {
        let value = self
            .root
            .get("mcpServers")
            .and_then(Value::as_object)
            .and_then(|servers| servers.get(name));
        value.map(canonical_json_digest).transpose()
    }

    pub fn set(&mut self, name: &str, target: &McpTarget) -> Result<()> {
        if !self.root.contains_key("mcpServers") {
            self.root
                .insert("mcpServers".into(), Value::Object(Map::new()));
        }
        let servers = self
            .root
            .get_mut("mcpServers")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| AruError::msg(".mcp.json mcpServers must be an object"))?;
        servers.insert(name.into(), normalized_entry(target)?);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) {
        if let Some(servers) = self
            .root
            .get_mut("mcpServers")
            .and_then(Value::as_object_mut)
        {
            servers.remove(name);
        }
    }

    pub fn bytes(&self) -> Result<Vec<u8>> {
        let mut bytes = serde_json::to_vec_pretty(&self.root)
            .map_err(|error| AruError::msg(format!("could not serialize .mcp.json: {error}")))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Agent;

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
                    agent: Agent::ClaudeCode,
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
