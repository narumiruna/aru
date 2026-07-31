pub mod claude;
pub mod codex;
pub mod copilot;
pub mod instructions;
pub(crate) mod mcp;
pub mod opencode;
pub(crate) mod skill;

use serde_json::{Map, Value, json};

use crate::digest::canonical_json_digest;
use crate::error::{AruError, Result};
use crate::lockfile::McpTarget;
use crate::manifest::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstructionCapability {
    NativeAgents,
    Claude,
    Copilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCapabilities {
    pub instructions: InstructionCapability,
    pub skills: bool,
    pub mcp: bool,
}

pub fn capabilities(target: Target) -> TargetCapabilities {
    match target {
        Target::Codex => TargetCapabilities {
            instructions: InstructionCapability::NativeAgents,
            skills: true,
            mcp: true,
        },
        Target::Claude => TargetCapabilities {
            instructions: InstructionCapability::Claude,
            skills: true,
            mcp: true,
        },
        Target::Copilot => TargetCapabilities {
            instructions: InstructionCapability::Copilot,
            skills: true,
            mcp: true,
        },
        Target::Opencode => TargetCapabilities {
            instructions: InstructionCapability::NativeAgents,
            skills: true,
            mcp: true,
        },
        Target::Pi => TargetCapabilities {
            instructions: InstructionCapability::NativeAgents,
            skills: true,
            mcp: false,
        },
    }
}

pub fn skill_targets(targets: &[Target]) -> Vec<Target> {
    targets
        .iter()
        .copied()
        .filter(|target| capabilities(*target).skills)
        .collect()
}

pub fn mcp_targets(targets: &[Target]) -> Vec<Target> {
    targets
        .iter()
        .copied()
        .filter(|target| capabilities(*target).mcp)
        .collect()
}

pub(crate) fn supports_mcp_candidate(
    target: Target,
    transport: &str,
    has_command: bool,
    has_url: bool,
) -> bool {
    match transport {
        "stdio" => capabilities(target).mcp && has_command,
        "streamable-http" => capabilities(target).mcp && has_url,
        _ => false,
    }
}

pub fn normalized_entry(target: &McpTarget) -> Result<Value> {
    match (target.target, target.transport.as_str()) {
        (Target::Codex, "stdio") => {
            let mut map = Map::new();
            map.insert("command".into(), json!(target.command));
            map.insert("args".into(), json!(target.args));
            map.insert("enabled".into(), json!(true));
            if !target.env_vars.is_empty() {
                map.insert("env_vars".into(), json!(target.env_vars));
            }
            Ok(Value::Object(map))
        }
        (Target::Codex, "streamable-http") => {
            let mut map = Map::new();
            map.insert("url".into(), json!(target.url));
            map.insert("enabled".into(), json!(true));
            if let Some(env) = &target.bearer_token_env {
                map.insert("bearer_token_env_var".into(), json!(env));
            }
            if !target.env_http_headers.is_empty() {
                map.insert("env_http_headers".into(), json!(target.env_http_headers));
            }
            Ok(Value::Object(map))
        }
        (Target::Claude, "stdio") => {
            let mut map = Map::new();
            map.insert("type".into(), json!("stdio"));
            map.insert("command".into(), json!(target.command));
            map.insert("args".into(), json!(target.args));
            if !target.env_vars.is_empty() {
                let environment: Map<String, Value> = target
                    .env_vars
                    .iter()
                    .map(|name| (name.clone(), json!(format!("${{{name}}}"))))
                    .collect();
                map.insert("env".into(), Value::Object(environment));
            }
            Ok(Value::Object(map))
        }
        (Target::Claude, "streamable-http") => {
            let mut map = Map::new();
            map.insert("type".into(), json!("http"));
            map.insert("url".into(), json!(target.url));
            let mut headers: Map<String, Value> = target
                .env_http_headers
                .iter()
                .map(|(header, env)| (header.clone(), json!(format!("${{{env}}}"))))
                .collect();
            if let Some(env) = &target.bearer_token_env {
                headers.insert("Authorization".into(), json!(format!("Bearer ${{{env}}}")));
            }
            if !headers.is_empty() {
                map.insert("headers".into(), Value::Object(headers));
            }
            Ok(Value::Object(map))
        }
        (Target::Copilot, "stdio") => {
            let mut map = Map::new();
            map.insert("type".into(), json!("stdio"));
            map.insert("command".into(), json!(target.command));
            map.insert("args".into(), json!(target.args));
            if !target.env_vars.is_empty() {
                let environment: Map<String, Value> = target
                    .env_vars
                    .iter()
                    .map(|name| (name.clone(), json!(format!("${{{name}}}"))))
                    .collect();
                map.insert("env".into(), Value::Object(environment));
            }
            map.insert("tools".into(), json!(["*"]));
            Ok(Value::Object(map))
        }
        (Target::Copilot, "streamable-http") => {
            let mut map = Map::new();
            map.insert("type".into(), json!("http"));
            map.insert("url".into(), json!(target.url));
            let mut headers: Map<String, Value> = target
                .env_http_headers
                .iter()
                .map(|(header, env)| (header.clone(), json!(format!("${{{env}}}"))))
                .collect();
            if let Some(env) = &target.bearer_token_env {
                headers.insert("Authorization".into(), json!(format!("Bearer ${{{env}}}")));
            }
            if !headers.is_empty() {
                map.insert("headers".into(), Value::Object(headers));
            }
            map.insert("tools".into(), json!(["*"]));
            Ok(Value::Object(map))
        }
        (Target::Opencode, "stdio") => {
            let mut map = Map::new();
            map.insert("type".into(), json!("local"));
            let command = target
                .command
                .iter()
                .chain(target.args.iter())
                .cloned()
                .collect::<Vec<_>>();
            map.insert("command".into(), json!(command));
            map.insert("enabled".into(), json!(true));
            if !target.env_vars.is_empty() {
                let environment: Map<String, Value> = target
                    .env_vars
                    .iter()
                    .map(|name| (name.clone(), json!(format!("{{env:{name}}}"))))
                    .collect();
                map.insert("environment".into(), Value::Object(environment));
            }
            Ok(Value::Object(map))
        }
        (Target::Opencode, "streamable-http") => {
            let mut map = Map::new();
            map.insert("type".into(), json!("remote"));
            map.insert("url".into(), json!(target.url));
            map.insert("enabled".into(), json!(true));
            let mut headers: Map<String, Value> = target
                .env_http_headers
                .iter()
                .map(|(header, env)| (header.clone(), json!(format!("{{env:{env}}}"))))
                .collect();
            if let Some(env) = &target.bearer_token_env {
                headers.insert(
                    "Authorization".into(),
                    json!(format!("Bearer {{env:{env}}}")),
                );
            }
            if !headers.is_empty() {
                map.insert("headers".into(), Value::Object(headers));
                map.insert("oauth".into(), json!(false));
            }
            Ok(Value::Object(map))
        }
        (_, transport) => Err(AruError::msg(format!(
            "unsupported MCP transport {transport:?} for {}",
            target.target
        ))),
    }
}

pub fn entry_digest(target: &McpTarget) -> Result<String> {
    canonical_json_digest(&normalized_entry(target)?)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn target(target: Target, transport: &str) -> McpTarget {
        McpTarget {
            target,
            kind: "test".into(),
            transport: transport.into(),
            command: (transport == "stdio").then(|| "demo".into()),
            args: if transport == "stdio" {
                vec!["--serve".into()]
            } else {
                Vec::new()
            },
            env_vars: if transport == "stdio" {
                vec!["DEMO_TOKEN".into()]
            } else {
                Vec::new()
            },
            env_http_headers: if transport == "streamable-http" {
                BTreeMap::from([("X-Demo".into(), "DEMO_HEADER".into())])
            } else {
                BTreeMap::new()
            },
            url: (transport == "streamable-http").then(|| "https://example.com/mcp".into()),
            bearer_token_env: (transport == "streamable-http").then(|| "DEMO_TOKEN".into()),
            package: None,
        }
    }

    #[test]
    fn copilot_entries_use_cli_project_format_and_environment_references() {
        assert_eq!(
            normalized_entry(&target(Target::Copilot, "stdio")).unwrap(),
            json!({
                "type": "stdio",
                "command": "demo",
                "args": ["--serve"],
                "env": {"DEMO_TOKEN": "${DEMO_TOKEN}"},
                "tools": ["*"]
            })
        );
        assert_eq!(
            normalized_entry(&target(Target::Copilot, "streamable-http")).unwrap(),
            json!({
                "type": "http",
                "url": "https://example.com/mcp",
                "headers": {
                    "Authorization": "Bearer ${DEMO_TOKEN}",
                    "X-Demo": "${DEMO_HEADER}"
                },
                "tools": ["*"]
            })
        );
    }

    #[test]
    fn opencode_entries_use_native_command_arrays_and_environment_references() {
        assert_eq!(
            normalized_entry(&target(Target::Opencode, "stdio")).unwrap(),
            json!({
                "type": "local",
                "command": ["demo", "--serve"],
                "enabled": true,
                "environment": {"DEMO_TOKEN": "{env:DEMO_TOKEN}"}
            })
        );
        assert_eq!(
            normalized_entry(&target(Target::Opencode, "streamable-http")).unwrap(),
            json!({
                "type": "remote",
                "url": "https://example.com/mcp",
                "enabled": true,
                "headers": {
                    "Authorization": "Bearer {env:DEMO_TOKEN}",
                    "X-Demo": "{env:DEMO_HEADER}"
                },
                "oauth": false
            })
        );
    }
}
