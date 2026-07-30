use std::path::Path;

use serde_json::{Map, Value};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Table};

use crate::agent::normalized_entry;
use crate::digest::canonical_json_digest;
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::McpTarget;

pub const CONFIG_PATH: &str = ".codex/config.toml";

#[derive(Debug, Clone)]
pub struct CodexConfig {
    doc: DocumentMut,
}

impl CodexConfig {
    pub fn load(project: &Path) -> Result<Self> {
        let path = project.join(CONFIG_PATH);
        let doc = if path.exists() {
            let text = std::fs::read_to_string(&path).at(&path)?;
            text.parse::<DocumentMut>().map_err(|error| {
                AruError::msg(format!("invalid TOML in {}: {error}", path.display()))
            })?
        } else {
            DocumentMut::new()
        };
        if doc.get("mcp_servers").is_some_and(|item| !item.is_table()) {
            return Err(AruError::msg(
                "Codex config mcp_servers must be a TOML table",
            ));
        }
        if let Some(servers) = doc.get("mcp_servers").and_then(Item::as_table) {
            for (name, item) in servers.iter() {
                if item
                    .as_table()
                    .is_some_and(|table| table.contains_key("bearer_token"))
                {
                    return Err(AruError::msg(format!(
                        "Codex MCP entry {name:?} contains an inline bearer_token; aru refuses inline secrets"
                    )));
                }
            }
        }
        Ok(Self { doc })
    }

    pub fn digest(&self, name: &str) -> Result<Option<String>> {
        let Some(item) = self
            .doc
            .get("mcp_servers")
            .and_then(Item::as_table)
            .and_then(|servers| servers.get(name))
        else {
            return Ok(None);
        };
        Ok(Some(canonical_json_digest(&item_to_json(item)?)?))
    }

    pub fn set(&mut self, name: &str, target: &McpTarget) -> Result<()> {
        if self.doc.get("mcp_servers").is_none() {
            self.doc["mcp_servers"] = Item::Table(Table::new());
        }
        let value = normalized_entry(target)?;
        self.doc["mcp_servers"][name] = Item::Table(json_object_to_table(&value)?);
        Ok(())
    }

    pub fn remove(&mut self, name: &str) {
        if let Some(servers) = self.doc["mcp_servers"].as_table_mut() {
            servers.remove(name);
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.doc.to_string().into_bytes()
    }
}

fn json_object_to_table(value: &Value) -> Result<Table> {
    let object = value
        .as_object()
        .ok_or_else(|| AruError::msg("MCP entry renderer did not return an object"))?;
    let mut table = Table::new();
    for (key, value) in object {
        table[key] = json_to_item(value)?;
    }
    Ok(table)
}

fn json_to_item(value: &Value) -> Result<Item> {
    Ok(match value {
        Value::Null => return Err(AruError::msg("MCP entry contains an unexpected null")),
        Value::Bool(value) => toml_edit::value(*value),
        Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                toml_edit::value(integer)
            } else if let Some(float) = value.as_f64() {
                toml_edit::value(float)
            } else {
                return Err(AruError::msg("unsupported MCP number"));
            }
        }
        Value::String(value) => toml_edit::value(value.as_str()),
        Value::Array(values) => {
            let mut array = Array::new();
            for value in values {
                match json_to_item(value)? {
                    Item::Value(value) => array.push_formatted(value),
                    _ => return Err(AruError::msg("nested MCP arrays are unsupported")),
                }
            }
            Item::Value(array.into())
        }
        Value::Object(values) => {
            let mut table = InlineTable::new();
            for (key, value) in values {
                match json_to_item(value)? {
                    Item::Value(value) => {
                        table.insert(key, value);
                    }
                    _ => return Err(AruError::msg("nested MCP tables are unsupported")),
                }
            }
            Item::Value(table.into())
        }
    })
}

fn item_to_json(item: &Item) -> Result<Value> {
    match item {
        Item::None => Ok(Value::Null),
        Item::Value(value) => value_to_json(value),
        Item::Table(table) => {
            let mut output = Map::new();
            for (key, item) in table.iter() {
                output.insert(key.into(), item_to_json(item)?);
            }
            Ok(Value::Object(output))
        }
        Item::ArrayOfTables(tables) => {
            let values = tables
                .iter()
                .map(|table| item_to_json(&Item::Table(table.clone())))
                .collect::<Result<Vec<_>>>()?;
            Ok(Value::Array(values))
        }
    }
}

fn value_to_json(value: &toml_edit::Value) -> Result<Value> {
    Ok(match value {
        toml_edit::Value::String(value) => Value::String(value.value().clone()),
        toml_edit::Value::Integer(value) => Value::Number((*value.value()).into()),
        toml_edit::Value::Float(value) => serde_json::Number::from_f64(*value.value())
            .map(Value::Number)
            .ok_or_else(|| AruError::msg("non-finite TOML float"))?,
        toml_edit::Value::Boolean(value) => Value::Bool(*value.value()),
        toml_edit::Value::Datetime(value) => Value::String(value.value().to_string()),
        toml_edit::Value::Array(array) => Value::Array(
            array
                .iter()
                .map(value_to_json)
                .collect::<Result<Vec<_>>>()?,
        ),
        toml_edit::Value::InlineTable(table) => {
            let mut output = Map::new();
            for (key, value) in table.iter() {
                output.insert(key.into(), value_to_json(value)?);
            }
            Value::Object(output)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Agent;

    #[test]
    fn merge_preserves_comments_and_unrelated_entries() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".codex")).unwrap();
        std::fs::write(
            project.path().join(CONFIG_PATH),
            "# keep\nmodel = \"x\"\n[mcp_servers.unmanaged]\ncommand = \"keep\"\n",
        )
        .unwrap();
        let mut config = CodexConfig::load(project.path()).unwrap();
        config
            .set(
                "managed",
                &McpTarget {
                    agent: Agent::Codex,
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
        let output = String::from_utf8(config.bytes()).unwrap();
        assert!(output.contains("# keep"));
        assert!(output.contains("[mcp_servers.unmanaged]"));
        assert!(output.contains("[mcp_servers.managed]"));
    }

    #[test]
    fn inline_bearer_secrets_fail_closed() {
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".codex")).unwrap();
        std::fs::write(
            project.path().join(CONFIG_PATH),
            "[mcp_servers.private]\nurl = \"https://example.com/mcp\"\nbearer_token = \"secret\"\n",
        )
        .unwrap();
        assert!(CodexConfig::load(project.path()).is_err());
    }
}
