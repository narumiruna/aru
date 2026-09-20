//! Shared object-backed MCP storage for Claude and Copilot.
//! Target paths and entry rendering remain the adapters' responsibility.

use std::path::Path;

use serde_json::{Map, Value};

use crate::digest::canonical_json_digest;
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::McpTarget;

pub(super) fn load(project: &Path, config_path: &str) -> Result<Map<String, Value>> {
    let path = project.join(config_path);
    let root = if path.exists() {
        let bytes = std::fs::read(&path).at(&path)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|source| AruError::Json {
            path: path.clone(),
            source,
        })?;
        value
            .as_object()
            .cloned()
            .ok_or_else(|| AruError::msg(format!("{config_path} root must be a JSON object")))?
    } else {
        Map::new()
    };
    if root
        .get("mcpServers")
        .is_some_and(|value| !value.is_object())
    {
        return Err(AruError::msg(format!(
            "{config_path} mcpServers must be an object"
        )));
    }
    Ok(root)
}

pub(super) fn digest(root: &Map<String, Value>, name: &str) -> Result<Option<String>> {
    let value = root
        .get("mcpServers")
        .and_then(Value::as_object)
        .and_then(|servers| servers.get(name));
    value.map(canonical_json_digest).transpose()
}

pub(super) fn set(
    root: &mut Map<String, Value>,
    config_path: &str,
    name: &str,
    target: &McpTarget,
) -> Result<()> {
    if !root.contains_key("mcpServers") {
        root.insert("mcpServers".into(), Value::Object(Map::new()));
    }
    let servers = root
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| AruError::msg(format!("{config_path} mcpServers must be an object")))?;
    servers.insert(name.into(), super::normalized_entry(target)?);
    Ok(())
}

pub(super) fn remove(root: &mut Map<String, Value>, name: &str) {
    if let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) {
        servers.remove(name);
    }
}

pub(super) fn bytes(root: &Map<String, Value>, config_path: &str) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(root)
        .map_err(|error| AruError::msg(format!("could not serialize {config_path}: {error}")))?;
    bytes.push(b'\n');
    Ok(bytes)
}
