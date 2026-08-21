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
    pub instructions: Option<InstructionCapability>,
    pub skills: bool,
    pub mcp: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetSpec {
    pub target: Target,
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub project_skills: &'static str,
    pub capabilities: TargetCapabilities,
}

const NATIVE_SKILLS: TargetCapabilities = TargetCapabilities {
    instructions: Some(InstructionCapability::NativeAgents),
    skills: true,
    mcp: false,
};
const NATIVE_SKILLS_MCP: TargetCapabilities = TargetCapabilities {
    instructions: Some(InstructionCapability::NativeAgents),
    skills: true,
    mcp: true,
};
const CLAUDE_CAPABILITIES: TargetCapabilities = TargetCapabilities {
    instructions: Some(InstructionCapability::Claude),
    skills: true,
    mcp: true,
};
const COPILOT_CAPABILITIES: TargetCapabilities = TargetCapabilities {
    instructions: Some(InstructionCapability::Copilot),
    skills: true,
    mcp: true,
};
const SKILLS_ONLY: TargetCapabilities = TargetCapabilities {
    instructions: None,
    skills: true,
    mcp: false,
};

macro_rules! target_spec {
    ($target:ident, $name:literal, [$($alias:literal),* $(,)?], $path:literal, $capabilities:expr) => {
        TargetSpec {
            target: Target::$target,
            name: $name,
            aliases: &[$($alias),*],
            project_skills: $path,
            capabilities: $capabilities,
        }
    };
}

pub const TARGET_SPECS: &[TargetSpec] = &[
    target_spec!(
        Agents,
        "agents",
        ["universal"],
        ".agents/skills",
        NATIVE_SKILLS
    ),
    target_spec!(Codex, "codex", [], ".agents/skills", NATIVE_SKILLS_MCP),
    target_spec!(
        Claude,
        "claude",
        ["claude-code"],
        ".claude/skills",
        CLAUDE_CAPABILITIES
    ),
    target_spec!(
        Copilot,
        "copilot",
        ["github-copilot"],
        ".github/skills",
        COPILOT_CAPABILITIES
    ),
    target_spec!(
        Opencode,
        "opencode",
        [],
        ".opencode/skills",
        NATIVE_SKILLS_MCP
    ),
    target_spec!(Pi, "pi", [], ".pi/skills", NATIVE_SKILLS),
    target_spec!(Amp, "amp", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(
        Antigravity,
        "antigravity",
        ["antigravity-cli"],
        ".agents/skills",
        SKILLS_ONLY
    ),
    target_spec!(Cline, "cline", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(Cursor, "cursor", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(Deepagents, "deepagents", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(Dexto, "dexto", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(Firebender, "firebender", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(
        Gemini,
        "gemini",
        ["gemini-cli"],
        ".agents/skills",
        SKILLS_ONLY
    ),
    target_spec!(
        Kimi,
        "kimi",
        ["kimi-code-cli"],
        ".agents/skills",
        SKILLS_ONLY
    ),
    target_spec!(Loaf, "loaf", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(
        Promptscript,
        "promptscript",
        [],
        ".agents/skills",
        SKILLS_ONLY
    ),
    target_spec!(Replit, "replit", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(Warp, "warp", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(Zed, "zed", [], ".agents/skills", SKILLS_ONLY),
    target_spec!(Adal, "adal", [], ".adal/skills", SKILLS_ONLY),
    target_spec!(
        AiderDesk,
        "aider-desk",
        [],
        ".aider-desk/skills",
        SKILLS_ONLY
    ),
    target_spec!(Astrbot, "astrbot", [], "data/skills", SKILLS_ONLY),
    target_spec!(
        Autohand,
        "autohand",
        ["autohand-code"],
        ".autohand/skills",
        SKILLS_ONLY
    ),
    target_spec!(Augment, "augment", [], ".augment/skills", SKILLS_ONLY),
    target_spec!(Bob, "bob", [], ".bob/skills", SKILLS_ONLY),
    target_spec!(Openclaw, "openclaw", [], "skills", SKILLS_ONLY),
    target_spec!(
        Codearts,
        "codearts",
        ["codearts-agent"],
        ".codeartsdoer/skills",
        SKILLS_ONLY
    ),
    target_spec!(Codebuddy, "codebuddy", [], ".codebuddy/skills", SKILLS_ONLY),
    target_spec!(Codemaker, "codemaker", [], ".codemaker/skills", SKILLS_ONLY),
    target_spec!(
        Codestudio,
        "codestudio",
        [],
        ".codestudio/skills",
        SKILLS_ONLY
    ),
    target_spec!(
        Commandcode,
        "commandcode",
        ["command-code"],
        ".commandcode/skills",
        SKILLS_ONLY
    ),
    target_spec!(Continue, "continue", [], ".continue/skills", SKILLS_ONLY),
    target_spec!(Cortex, "cortex", [], ".cortex/skills", SKILLS_ONLY),
    target_spec!(Crush, "crush", [], ".crush/skills", SKILLS_ONLY),
    target_spec!(Devin, "devin", [], ".devin/skills", SKILLS_ONLY),
    target_spec!(Droid, "droid", [], ".factory/skills", SKILLS_ONLY),
    target_spec!(Eve, "eve", [], "agent/skills", SKILLS_ONLY),
    target_spec!(Forge, "forge", ["forgecode"], ".forge/skills", SKILLS_ONLY),
    target_spec!(Goose, "goose", [], ".goose/skills", SKILLS_ONLY),
    target_spec!(Grok, "grok", [], ".grok/skills", SKILLS_ONLY),
    target_spec!(
        Hermes,
        "hermes",
        ["hermes-agent"],
        ".hermes/skills",
        SKILLS_ONLY
    ),
    target_spec!(
        InferenceSh,
        "inference-sh",
        [],
        ".inferencesh/skills",
        SKILLS_ONLY
    ),
    target_spec!(Jazz, "jazz", [], ".jazz/skills", SKILLS_ONLY),
    target_spec!(Junie, "junie", [], ".junie/skills", SKILLS_ONLY),
    target_spec!(Iflow, "iflow", ["iflow-cli"], ".iflow/skills", SKILLS_ONLY),
    target_spec!(Kilo, "kilo", [], ".kilocode/skills", SKILLS_ONLY),
    target_spec!(Kimchi, "kimchi", [], ".kimchi/skills", SKILLS_ONLY),
    target_spec!(Kiro, "kiro", ["kiro-cli"], ".kiro/skills", SKILLS_ONLY),
    target_spec!(Kode, "kode", [], ".kode/skills", SKILLS_ONLY),
    target_spec!(Lingma, "lingma", [], ".lingma/skills", SKILLS_ONLY),
    target_spec!(Mcpjam, "mcpjam", [], ".mcpjam/skills", SKILLS_ONLY),
    target_spec!(
        Minimax,
        "minimax",
        ["minimax-code"],
        ".minimax/skills",
        SKILLS_ONLY
    ),
    target_spec!(Vibe, "vibe", ["mistral-vibe"], ".vibe/skills", SKILLS_ONLY),
    target_spec!(Moxby, "moxby", [], ".moxby/skills", SKILLS_ONLY),
    target_spec!(Mux, "mux", [], ".mux/skills", SKILLS_ONLY),
    target_spec!(Openhands, "openhands", [], ".openhands/skills", SKILLS_ONLY),
    target_spec!(Ona, "ona", [], ".ona/skills", SKILLS_ONLY),
    target_spec!(
        Posit,
        "posit",
        ["posit-assistant"],
        ".posit/assistant/skills",
        SKILLS_ONLY
    ),
    target_spec!(Qoder, "qoder", ["qoder-cn"], ".qoder/skills", SKILLS_ONLY),
    target_spec!(Qwen, "qwen", ["qwen-code"], ".qwen/skills", SKILLS_ONLY),
    target_spec!(Reasonix, "reasonix", [], ".reasonix/skills", SKILLS_ONLY),
    target_spec!(Rovodev, "rovodev", [], ".rovodev/skills", SKILLS_ONLY),
    target_spec!(Roo, "roo", [], ".roo/skills", SKILLS_ONLY),
    target_spec!(
        Tabnine,
        "tabnine",
        ["tabnine-cli"],
        ".tabnine/agent/skills",
        SKILLS_ONLY
    ),
    target_spec!(Terramind, "terramind", [], ".terramind/skills", SKILLS_ONLY),
    target_spec!(Tinycloud, "tinycloud", [], ".tinycloud/skills", SKILLS_ONLY),
    target_spec!(Trae, "trae", ["trae-cn"], ".trae/skills", SKILLS_ONLY),
    target_spec!(Windsurf, "windsurf", [], ".windsurf/skills", SKILLS_ONLY),
    target_spec!(Zcode, "zcode", [], ".zcode/skills", SKILLS_ONLY),
    target_spec!(
        Zencoder,
        "zencoder",
        ["zenflow"],
        ".zencoder/skills",
        SKILLS_ONLY
    ),
    target_spec!(Neovate, "neovate", [], ".neovate/skills", SKILLS_ONLY),
    target_spec!(Pochi, "pochi", [], ".pochi/skills", SKILLS_ONLY),
];

pub fn specs() -> &'static [TargetSpec] {
    TARGET_SPECS
}

pub fn spec(target: Target) -> &'static TargetSpec {
    TARGET_SPECS
        .iter()
        .find(|spec| spec.target == target)
        .expect("every target has a registry entry")
}

pub fn parse(value: &str) -> Option<Target> {
    TARGET_SPECS
        .iter()
        .find(|spec| spec.name == value || spec.aliases.contains(&value))
        .map(|spec| spec.target)
}

pub fn capabilities(target: Target) -> TargetCapabilities {
    spec(target).capabilities
}

pub fn instruction_targets(targets: &[Target]) -> Vec<Target> {
    targets
        .iter()
        .copied()
        .filter(|target| capabilities(*target).instructions.is_some())
        .collect()
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
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Component, Path};

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
    fn registry_names_aliases_and_paths_are_safe_and_unambiguous() {
        assert_eq!(TARGET_SPECS.len(), 73);
        let mut targets = BTreeSet::new();
        let mut inputs = BTreeSet::new();
        for spec in TARGET_SPECS {
            assert!(
                targets.insert(spec.target),
                "duplicate target {}",
                spec.name
            );
            assert!(inputs.insert(spec.name), "duplicate name {}", spec.name);
            assert_eq!(parse(spec.name), Some(spec.target));
            assert_eq!(
                serde_json::to_string(&spec.target).unwrap(),
                format!("\"{}\"", spec.name)
            );
            assert_eq!(
                serde_json::from_str::<Target>(&format!("\"{}\"", spec.name)).unwrap(),
                spec.target
            );
            assert!(
                spec.name
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            );
            let path = Path::new(spec.project_skills);
            assert!(!path.is_absolute());
            assert!(
                path.components()
                    .all(|component| matches!(component, Component::Normal(_)))
            );
            assert_ne!(
                path.components().next(),
                Some(Component::Normal(".aru".as_ref()))
            );
            assert_eq!(
                path.file_name().and_then(|name| name.to_str()),
                Some("skills")
            );
            for alias in spec.aliases {
                assert!(inputs.insert(alias), "duplicate alias {alias}");
                assert_eq!(parse(alias), Some(spec.target));
            }
        }
    }

    #[test]
    fn aliases_normalize_to_canonical_target_names() {
        for (alias, target, canonical) in [
            ("claude-code", Target::Claude, "claude"),
            ("kiro-cli", Target::Kiro, "kiro"),
            ("hermes-agent", Target::Hermes, "hermes"),
            ("github-copilot", Target::Copilot, "copilot"),
            ("cursor", Target::Cursor, "cursor"),
        ] {
            assert_eq!(parse(alias), Some(target));
            assert_eq!(target.to_string(), canonical);
        }
    }

    #[test]
    fn pinned_vercel_names_map_to_the_approved_canonical_registry() {
        let mappings = [
            ("aider-desk", "aider-desk", ".aider-desk/skills"),
            ("amp", "amp", ".agents/skills"),
            ("antigravity", "antigravity", ".agents/skills"),
            ("antigravity-cli", "antigravity", ".agents/skills"),
            ("astrbot", "astrbot", "data/skills"),
            ("autohand-code", "autohand", ".autohand/skills"),
            ("augment", "augment", ".augment/skills"),
            ("bob", "bob", ".bob/skills"),
            ("claude-code", "claude", ".claude/skills"),
            ("openclaw", "openclaw", "skills"),
            ("cline", "cline", ".agents/skills"),
            ("codearts-agent", "codearts", ".codeartsdoer/skills"),
            ("codebuddy", "codebuddy", ".codebuddy/skills"),
            ("codemaker", "codemaker", ".codemaker/skills"),
            ("codestudio", "codestudio", ".codestudio/skills"),
            ("codex", "codex", ".agents/skills"),
            ("command-code", "commandcode", ".commandcode/skills"),
            ("continue", "continue", ".continue/skills"),
            ("cortex", "cortex", ".cortex/skills"),
            ("crush", "crush", ".crush/skills"),
            ("cursor", "cursor", ".agents/skills"),
            ("deepagents", "deepagents", ".agents/skills"),
            ("devin", "devin", ".devin/skills"),
            ("dexto", "dexto", ".agents/skills"),
            ("droid", "droid", ".factory/skills"),
            ("eve", "eve", "agent/skills"),
            ("firebender", "firebender", ".agents/skills"),
            ("forgecode", "forge", ".forge/skills"),
            ("gemini-cli", "gemini", ".agents/skills"),
            ("github-copilot", "copilot", ".github/skills"),
            ("goose", "goose", ".goose/skills"),
            ("grok", "grok", ".grok/skills"),
            ("hermes-agent", "hermes", ".hermes/skills"),
            ("inference-sh", "inference-sh", ".inferencesh/skills"),
            ("jazz", "jazz", ".jazz/skills"),
            ("junie", "junie", ".junie/skills"),
            ("iflow-cli", "iflow", ".iflow/skills"),
            ("kilo", "kilo", ".kilocode/skills"),
            ("kimchi", "kimchi", ".kimchi/skills"),
            ("kimi-code-cli", "kimi", ".agents/skills"),
            ("kiro-cli", "kiro", ".kiro/skills"),
            ("kode", "kode", ".kode/skills"),
            ("lingma", "lingma", ".lingma/skills"),
            ("loaf", "loaf", ".agents/skills"),
            ("mcpjam", "mcpjam", ".mcpjam/skills"),
            ("minimax-code", "minimax", ".minimax/skills"),
            ("mistral-vibe", "vibe", ".vibe/skills"),
            ("moxby", "moxby", ".moxby/skills"),
            ("mux", "mux", ".mux/skills"),
            ("opencode", "opencode", ".opencode/skills"),
            ("openhands", "openhands", ".openhands/skills"),
            ("ona", "ona", ".ona/skills"),
            ("pi", "pi", ".pi/skills"),
            ("posit-assistant", "posit", ".posit/assistant/skills"),
            ("qoder", "qoder", ".qoder/skills"),
            ("qoder-cn", "qoder", ".qoder/skills"),
            ("qwen-code", "qwen", ".qwen/skills"),
            ("replit", "replit", ".agents/skills"),
            ("reasonix", "reasonix", ".reasonix/skills"),
            ("rovodev", "rovodev", ".rovodev/skills"),
            ("roo", "roo", ".roo/skills"),
            ("tabnine-cli", "tabnine", ".tabnine/agent/skills"),
            ("terramind", "terramind", ".terramind/skills"),
            ("tinycloud", "tinycloud", ".tinycloud/skills"),
            ("trae", "trae", ".trae/skills"),
            ("trae-cn", "trae", ".trae/skills"),
            ("warp", "warp", ".agents/skills"),
            ("windsurf", "windsurf", ".windsurf/skills"),
            ("zed", "zed", ".agents/skills"),
            ("zcode", "zcode", ".zcode/skills"),
            ("zencoder", "zencoder", ".zencoder/skills"),
            ("zenflow", "zencoder", ".zencoder/skills"),
            ("neovate", "neovate", ".neovate/skills"),
            ("pochi", "pochi", ".pochi/skills"),
            ("promptscript", "promptscript", ".agents/skills"),
            ("adal", "adal", ".adal/skills"),
            ("universal", "agents", ".agents/skills"),
        ];
        assert_eq!(mappings.len(), 77);
        for (upstream, canonical, project_skills) in mappings {
            let target =
                parse(upstream).unwrap_or_else(|| panic!("missing mapping for {upstream}"));
            assert_eq!(target.to_string(), canonical);
            assert_eq!(spec(target).project_skills, project_skills);
        }
    }

    #[test]
    fn agents_supports_native_instructions_and_skills_without_mcp() {
        assert_eq!(
            capabilities(Target::Agents),
            TargetCapabilities {
                instructions: Some(InstructionCapability::NativeAgents),
                skills: true,
                mcp: false,
            }
        );
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
