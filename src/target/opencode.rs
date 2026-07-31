use std::collections::BTreeSet;
use std::path::Path;

use jsonc_parser::ParseOptions;
use jsonc_parser::cst::{CstInputValue, CstObject, CstRootNode};
use serde_json::Value;

use crate::digest::canonical_json_digest;
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::McpTarget;
use crate::target::normalized_entry;

pub const CONFIG_PATH: &str = "opencode.json";

#[derive(Debug, Clone)]
pub struct OpencodeConfig {
    root: CstRootNode,
}

impl OpencodeConfig {
    pub fn load(project: &Path) -> Result<Self> {
        let path = project.join(CONFIG_PATH);
        let text = if path.exists() {
            std::fs::read_to_string(&path).at(&path)?
        } else {
            "{}\n".into()
        };
        let root = CstRootNode::parse(&text, &parse_options())
            .map_err(|error| AruError::msg(format!("invalid JSONC in {CONFIG_PATH}: {error}")))?;
        let object = root
            .object_value()
            .ok_or_else(|| AruError::msg(format!("{CONFIG_PATH} root must be a JSON object")))?;
        validate_unique_properties(&object, "opencode.json root")?;
        if object.get("mcp").is_some() && object.object_value("mcp").is_none() {
            return Err(AruError::msg("opencode.json mcp must be an object"));
        }
        if let Some(mcp) = object.object_value("mcp") {
            validate_unique_properties(&mcp, "opencode.json mcp")?;
        }
        Ok(Self { root })
    }

    pub fn digest(&self, name: &str) -> Result<Option<String>> {
        let value = self
            .root
            .object_value()
            .and_then(|root| root.object_value("mcp"))
            .and_then(|servers| servers.get(name))
            .and_then(|entry| entry.to_serde_value());
        value.as_ref().map(canonical_json_digest).transpose()
    }

    pub fn set(&mut self, name: &str, target: &McpTarget) -> Result<()> {
        let root = self
            .root
            .object_value()
            .expect("validated OpenCode config has an object root");
        let servers = root
            .object_value_or_create("mcp")
            .ok_or_else(|| AruError::msg("opencode.json mcp must be an object"))?;
        let value = json_to_cst(normalized_entry(target)?)?;
        if let Some(entry) = servers.get(name) {
            entry.set_value(value);
        } else {
            servers.append(name, value);
        }
        Ok(())
    }

    pub fn remove(&mut self, name: &str) {
        if let Some(entry) = self
            .root
            .object_value()
            .and_then(|root| root.object_value("mcp"))
            .and_then(|servers| servers.get(name))
        {
            entry.remove();
        }
    }

    pub fn bytes(&self) -> Result<Vec<u8>> {
        Ok(self.root.to_string().into_bytes())
    }
}

fn validate_unique_properties(object: &CstObject, context: &str) -> Result<()> {
    let mut names = BTreeSet::new();
    for property in object.properties() {
        let name = property
            .name()
            .ok_or_else(|| AruError::msg(format!("{context} contains a property without a name")))?
            .decoded_value()
            .map_err(|error| {
                AruError::msg(format!("invalid property name in {context}: {error}"))
            })?;
        if !names.insert(name.clone()) {
            return Err(AruError::msg(format!(
                "{context} contains duplicate property {name:?}"
            )));
        }
    }
    Ok(())
}

fn parse_options() -> ParseOptions {
    ParseOptions {
        allow_comments: true,
        allow_loose_object_property_names: false,
        allow_trailing_commas: true,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
    }
}

fn json_to_cst(value: Value) -> Result<CstInputValue> {
    Ok(match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(value) => CstInputValue::Bool(value),
        Value::Number(value) => CstInputValue::Number(value.to_string()),
        Value::String(value) => CstInputValue::String(value),
        Value::Array(values) => CstInputValue::Array(
            values
                .into_iter()
                .map(json_to_cst)
                .collect::<Result<Vec<_>>>()?,
        ),
        Value::Object(values) => CstInputValue::Object(
            values
                .into_iter()
                .map(|(key, value)| Ok((key, json_to_cst(value)?)))
                .collect::<Result<Vec<_>>>()?,
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Target;

    #[test]
    fn merge_preserves_jsonc_comments_and_unrelated_servers() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(
            project.path().join(CONFIG_PATH),
            r#"{
  // keep this comment
  "mcp": {
    "unmanaged": { "type": "local", "command": ["keep"] },
  },
}
"#,
        )
        .unwrap();
        let mut config = OpencodeConfig::load(project.path()).unwrap();
        config
            .set(
                "managed",
                &McpTarget {
                    target: Target::Opencode,
                    kind: "command".into(),
                    transport: "stdio".into(),
                    command: Some("uvx".into()),
                    args: vec!["demo@1".into()],
                    env_vars: Vec::new(),
                    env_http_headers: std::collections::BTreeMap::new(),
                    url: None,
                    bearer_token_env: None,
                    package: None,
                },
            )
            .unwrap();
        let output = String::from_utf8(config.bytes().unwrap()).unwrap();
        assert!(output.contains("// keep this comment"));
        assert!(output.contains("\"unmanaged\""));
        assert!(output.contains("\"managed\""));
        assert!(output.contains("\"uvx\""));

        config.remove("managed");
        let output = String::from_utf8(config.bytes().unwrap()).unwrap();
        assert!(output.contains("// keep this comment"));
        assert!(output.contains("\"unmanaged\""));
        assert!(!output.contains("\"managed\":"));
    }

    #[test]
    fn non_object_and_duplicate_mcp_configuration_fails_closed() {
        let project = tempfile::tempdir().unwrap();
        std::fs::write(project.path().join(CONFIG_PATH), r#"{"mcp": []}"#).unwrap();
        assert!(OpencodeConfig::load(project.path()).is_err());

        std::fs::write(
            project.path().join(CONFIG_PATH),
            r#"{"mcp": {"demo": {}, "demo": {}}}"#,
        )
        .unwrap();
        assert!(OpencodeConfig::load(project.path()).is_err());
    }
}
