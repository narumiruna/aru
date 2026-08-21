pub mod resolver;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AruError, IoContext, Result};
use crate::manifest::{McpRequirement, PluginFormat};
use crate::package::{TreeBudget, tree_digest, validate_tree};

pub const AGENT_PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
pub const AGENT_MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";
pub const ADAPTER_VERSION: u32 = 1;
const MANIFEST_MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContributingManifest {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventorySkill {
    pub name: String,
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryMcp {
    pub name: String,
    pub requirement: Option<McpRequirement>,
    pub issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInventory {
    pub name: String,
    pub declared_version: Option<String>,
    pub description: Option<String>,
    pub format: PluginFormat,
    pub manifests: Vec<ContributingManifest>,
    pub tree_sha256: String,
    pub skills: Vec<InventorySkill>,
    pub mcp: Vec<InventoryMcp>,
    pub unsupported: Vec<String>,
    pub diagnostics: Vec<String>,
    pub root: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
struct PortableManifest {
    #[serde(rename = "$schema")]
    schema: String,
    name: String,
    #[serde(default)]
    version: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    extensions: Option<serde_json::Value>,
}

pub fn plugin_root(checkout: &Path, subdir: Option<&str>) -> Result<PathBuf> {
    let checkout = checkout.canonicalize().at(checkout)?;
    let relative = match subdir {
        None => PathBuf::new(),
        Some(value) => crate::skill::validate_relative_selector(value)?,
    };
    let selected = checkout.join(relative);
    let metadata = std::fs::symlink_metadata(&selected).at(&selected)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AruError::msg("plugin root must be a regular directory"));
    }
    let selected = selected.canonicalize().at(&selected)?;
    if !selected.starts_with(&checkout) {
        return Err(AruError::msg("plugin --subdir escapes the Git checkout"));
    }
    Ok(selected)
}

pub fn inspect_plugin_root(root: &Path, explicit: Option<PluginFormat>) -> Result<PluginInventory> {
    let root = root.canonicalize().at(root)?;
    let mut budget = TreeBudget::default();
    validate_tree(&root, &mut budget)?;
    let format = detect_format(&root, explicit)?;
    let mut inventory = match format {
        PluginFormat::AgentPlugins => inspect_agent_plugins(&root, false)?,
        PluginFormat::Openai => inspect_openai(&root)?,
        PluginFormat::Gemini => inspect_gemini(&root)?,
    };
    inventory.tree_sha256 = tree_digest(&root)?;
    inventory.manifests.sort_by(|a, b| a.path.cmp(&b.path));
    inventory.skills.sort_by(|a, b| a.name.cmp(&b.name));
    inventory.mcp.sort_by(|a, b| a.name.cmp(&b.name));
    inventory.unsupported.sort();
    inventory.unsupported.dedup();
    inventory.diagnostics.sort();
    inventory.diagnostics.dedup();
    Ok(inventory)
}

pub fn detect_format(root: &Path, explicit: Option<PluginFormat>) -> Result<PluginFormat> {
    if let Some(format) = explicit {
        validate_explicit_manifest(root, format)?;
        return Ok(format);
    }
    let portable = portable_schema(root)?;
    if let Some(schema) = portable.as_deref()
        && schema.contains("agent-plugins.org/schemas/")
        && schema != AGENT_PLUGIN_SCHEMA
    {
        return Err(AruError::msg(format!(
            "unsupported Agent Plugins schema {schema:?}; aru supports only 1.0.0"
        )));
    }
    let agent = portable.as_deref() == Some(AGENT_PLUGIN_SCHEMA);
    let openai = root.join(".codex-plugin/plugin.json").is_file()
        || agent && portable_extension(root, "com.openai")?.is_some();
    let gemini = root.join("gemini-extension.json").is_file();
    match (agent, openai, gemini) {
        (true, true, false) => Ok(PluginFormat::Openai),
        (true, false, false) => Ok(PluginFormat::AgentPlugins),
        (false, true, false) => Ok(PluginFormat::Openai),
        (false, false, true) => Ok(PluginFormat::Gemini),
        (false, false, false) => Err(AruError::msg(
            "plugin root has no supported manifest; expected canonical plugin.json, .codex-plugin/plugin.json, or gemini-extension.json",
        )),
        _ => {
            let mut formats = Vec::new();
            if agent || openai {
                formats.push(if openai { "openai" } else { "agent-plugins" });
            }
            if gemini {
                formats.push("gemini");
            }
            Err(AruError::msg(format!(
                "ambiguous plugin formats {}; pass one of {}",
                formats.join(", "),
                formats
                    .iter()
                    .map(|format| format!("--format {format}"))
                    .collect::<Vec<_>>()
                    .join(" or ")
            )))
        }
    }
}

fn validate_explicit_manifest(root: &Path, format: PluginFormat) -> Result<()> {
    match format {
        PluginFormat::AgentPlugins => {
            let schema = portable_schema(root)?
                .ok_or_else(|| AruError::msg("--format agent-plugins requires root plugin.json"))?;
            if schema != AGENT_PLUGIN_SCHEMA {
                return Err(AruError::msg(format!(
                    "--format agent-plugins requires schema {AGENT_PLUGIN_SCHEMA:?}"
                )));
            }
        }
        PluginFormat::Openai => {
            let portable = portable_schema(root)?.as_deref() == Some(AGENT_PLUGIN_SCHEMA);
            if !portable && !root.join(".codex-plugin/plugin.json").is_file() {
                return Err(AruError::msg(
                    "--format openai requires .codex-plugin/plugin.json or an Agent Plugins 1.0 base",
                ));
            }
        }
        PluginFormat::Gemini => {
            if !root.join("gemini-extension.json").is_file() {
                return Err(AruError::msg(
                    "--format gemini requires gemini-extension.json",
                ));
            }
        }
    }
    Ok(())
}

fn inspect_agent_plugins(root: &Path, openai: bool) -> Result<PluginInventory> {
    let path = root.join("plugin.json");
    let value = read_json(&path)?;
    let manifest: PortableManifest = serde_json::from_value(value.clone()).map_err(|error| {
        AruError::msg(format!(
            "invalid Agent Plugins manifest plugin.json: {error}"
        ))
    })?;
    if manifest.schema != AGENT_PLUGIN_SCHEMA {
        return Err(AruError::msg(format!(
            "unsupported Agent Plugins schema {:?}; aru supports only 1.0.0",
            manifest.schema
        )));
    }
    validate_portable_name(&manifest.name)?;
    validate_portable_manifest_shape(&value)?;
    let mut diagnostics = unknown_portable_fields(&value);
    if manifest
        .extensions
        .as_ref()
        .is_some_and(|value| !value.is_object())
    {
        diagnostics.push("ignored non-object extensions field".into());
    }
    let (skills, skill_diagnostics) = discover_skills(root, "skills")?;
    diagnostics.extend(skill_diagnostics);
    let (mcp, mcp_diagnostics, mcp_manifest) = inspect_mcp_file(root, "mcp.json", true)?;
    diagnostics.extend(mcp_diagnostics);
    let mut manifests = vec![manifest_record(root, "plugin.json")?];
    if let Some(record) = mcp_manifest {
        manifests.push(record);
    }
    Ok(PluginInventory {
        name: manifest.name,
        declared_version: manifest.version,
        description: manifest.description,
        format: if openai {
            PluginFormat::Openai
        } else {
            PluginFormat::AgentPlugins
        },
        manifests,
        tree_sha256: String::new(),
        skills,
        mcp,
        unsupported: Vec::new(),
        diagnostics,
        root: root.to_path_buf(),
    })
}

fn inspect_openai(root: &Path) -> Result<PluginInventory> {
    if portable_schema(root)?.as_deref() == Some(AGENT_PLUGIN_SCHEMA) {
        let mut inventory = inspect_agent_plugins(root, true)?;
        if let Some(extension) = portable_extension(root, "com.openai")? {
            inventory
                .manifests
                .push(manifest_record(root, "plugin.json")?);
            inventory.unsupported.extend(openai_unsupported(&extension));
        } else if root.join(".codex-plugin/plugin.json").is_file() {
            let overlay = read_json(&root.join(".codex-plugin/plugin.json"))?;
            inventory
                .manifests
                .push(manifest_record(root, ".codex-plugin/plugin.json")?);
            inventory.unsupported.extend(openai_unsupported(&overlay));
        }
        inventory.unsupported.extend(fixed_unsupported(
            root,
            &[
                ("hooks", "openai:hooks"),
                ("apps", "openai:apps"),
                ("commands", "openai:commands"),
            ],
        ));
        inventory.manifests.sort_by(|a, b| a.path.cmp(&b.path));
        inventory.manifests.dedup_by(|a, b| a.path == b.path);
        return Ok(inventory);
    }

    let relative = ".codex-plugin/plugin.json";
    let value = read_json(&root.join(relative))?;
    let object = value
        .as_object()
        .ok_or_else(|| AruError::msg("OpenAI plugin manifest must be a JSON object"))?;
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| root.file_name().and_then(|name| name.to_str()))
        .ok_or_else(|| AruError::msg("OpenAI plugin has no usable name"))?
        .to_owned();
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let description = object
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let mut skill_roots = vec!["skills".to_owned()];
    if object.get("skills").is_some() {
        skill_roots.extend(declared_paths(object.get("skills"), "skills")?);
    }
    skill_roots.sort();
    skill_roots.dedup();
    let mut skills = Vec::new();
    let mut diagnostics = Vec::new();
    for skill_root in skill_roots {
        let (mut found, mut issues) = discover_skills(root, &skill_root)?;
        skills.append(&mut found);
        diagnostics.append(&mut issues);
    }
    reject_duplicate_skills(&skills)?;
    let (mcp, mut mcp_diagnostics, mcp_manifests) = inspect_openai_mcp(root, object)?;
    diagnostics.append(&mut mcp_diagnostics);
    let mut manifests = vec![manifest_record(root, relative)?];
    manifests.extend(mcp_manifests);
    Ok(PluginInventory {
        name,
        declared_version: version,
        description,
        format: PluginFormat::Openai,
        manifests,
        tree_sha256: String::new(),
        skills,
        mcp,
        unsupported: openai_unsupported(&value)
            .into_iter()
            .chain(fixed_unsupported(
                root,
                &[
                    ("hooks", "openai:hooks"),
                    ("apps", "openai:apps"),
                    ("commands", "openai:commands"),
                ],
            ))
            .collect(),
        diagnostics,
        root: root.to_path_buf(),
    })
}

fn inspect_gemini(root: &Path) -> Result<PluginInventory> {
    let relative = "gemini-extension.json";
    let value = read_json(&root.join(relative))?;
    let object = value
        .as_object()
        .ok_or_else(|| AruError::msg("Gemini extension manifest must be a JSON object"))?;
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AruError::msg("gemini-extension.json requires a non-empty name"))?
        .to_owned();
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AruError::msg("gemini-extension.json requires a non-empty version"))?
        .to_owned();
    let description = object
        .get("description")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    let (skills, mut diagnostics) = discover_skills(root, "skills")?;
    let mcp = object
        .get("mcpServers")
        .map(|value| parse_mcp_map(value, false))
        .transpose()?
        .unwrap_or_default();
    let unsupported = [
        "contextFileName",
        "settings",
        "experimentalSettings",
        "excludeTools",
        "commands",
        "hooks",
        "agents",
        "subagents",
        "policies",
        "themes",
    ]
    .into_iter()
    .filter(|key| active(object.get(*key)))
    .map(|key| format!("gemini:{key}"))
    .chain(fixed_unsupported(
        root,
        &[
            ("GEMINI.md", "gemini:context"),
            ("commands", "gemini:commands"),
            ("hooks", "gemini:hooks"),
            ("agents", "gemini:agents"),
            ("policies", "gemini:policies"),
            ("themes", "gemini:themes"),
        ],
    ))
    .collect();
    diagnostics.sort();
    Ok(PluginInventory {
        name,
        declared_version: Some(version),
        description,
        format: PluginFormat::Gemini,
        manifests: vec![manifest_record(root, relative)?],
        tree_sha256: String::new(),
        skills,
        mcp,
        unsupported,
        diagnostics,
        root: root.to_path_buf(),
    })
}

fn inspect_openai_mcp(
    root: &Path,
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<(Vec<InventoryMcp>, Vec<String>, Vec<ContributingManifest>)> {
    let mut mcp = Vec::new();
    let mut diagnostics = Vec::new();
    let mut manifests = Vec::new();
    let mut loaded_paths = std::collections::BTreeSet::new();
    for candidate in [".mcp.json", "mcp.json"] {
        if root.join(candidate).is_file() {
            let (mut found, mut issues, record) = inspect_mcp_file(root, candidate, false)?;
            mcp.append(&mut found);
            diagnostics.append(&mut issues);
            manifests.extend(record);
            loaded_paths.insert(candidate.to_owned());
        }
    }
    if let Some(value) = object.get("mcpServers") {
        if let Some(path) = value.as_str() {
            let relative = contained_relative(path)?;
            if loaded_paths.insert(relative.clone()) {
                let (mut found, mut issues, record) = inspect_mcp_file(root, &relative, false)?;
                mcp.append(&mut found);
                diagnostics.append(&mut issues);
                manifests.extend(record);
            }
        } else {
            mcp.extend(parse_mcp_map(value, false)?);
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for server in &mcp {
        if !names.insert(&server.name) {
            return Err(AruError::msg(format!(
                "duplicate OpenAI MCP name {:?}",
                server.name
            )));
        }
    }
    Ok((mcp, diagnostics, manifests))
}

fn inspect_mcp_file(
    root: &Path,
    relative: &str,
    agent_schema: bool,
) -> Result<(Vec<InventoryMcp>, Vec<String>, Option<ContributingManifest>)> {
    let path = root.join(relative);
    if !path.exists() {
        return Ok((Vec::new(), Vec::new(), None));
    }
    let value = read_json(&path)?;
    let object = value
        .as_object()
        .ok_or_else(|| AruError::msg(format!("{relative} must be a JSON object")))?;
    let mut diagnostics = Vec::new();
    if agent_schema {
        if object.get("$schema").and_then(serde_json::Value::as_str) != Some(AGENT_MCP_SCHEMA) {
            diagnostics.push(format!(
                "disabled invalid {relative}: unsupported or missing $schema"
            ));
            return Ok((
                Vec::new(),
                diagnostics,
                Some(manifest_record(root, relative)?),
            ));
        }
        if object
            .keys()
            .any(|key| key != "$schema" && key != "mcpServers")
        {
            diagnostics.push(format!(
                "disabled invalid {relative}: unknown top-level field"
            ));
            return Ok((
                Vec::new(),
                diagnostics,
                Some(manifest_record(root, relative)?),
            ));
        }
    }
    let servers = object.get("mcpServers").unwrap_or(&value);
    let parsed = match parse_mcp_map(servers, agent_schema) {
        Ok(parsed) => parsed,
        Err(error) if agent_schema => {
            diagnostics.push(format!("disabled invalid {relative}: {error}"));
            Vec::new()
        }
        Err(error) => return Err(error),
    };
    Ok((parsed, diagnostics, Some(manifest_record(root, relative)?)))
}

fn parse_mcp_map(value: &serde_json::Value, require_type: bool) -> Result<Vec<InventoryMcp>> {
    let object = value
        .as_object()
        .ok_or_else(|| AruError::msg("MCP servers declaration must be an object"))?;
    let mut output = Vec::new();
    for (name, value) in object {
        if crate::manifest::validate_name(name, "plugin MCP name").is_err() {
            output.push(InventoryMcp {
                name: name.clone(),
                requirement: None,
                issue: Some("invalid MCP name".into()),
            });
            continue;
        }
        if require_type {
            let transport = value.get("type").and_then(serde_json::Value::as_str);
            if !matches!(transport, Some("stdio" | "streamable-http" | "sse")) {
                output.push(InventoryMcp {
                    name: name.clone(),
                    requirement: None,
                    issue: Some("Agent Plugins MCP entry requires a recognized type".into()),
                });
                continue;
            }
        }
        match safe_mcp(name, value) {
            Ok(requirement) => output.push(InventoryMcp {
                name: name.clone(),
                requirement: Some(requirement),
                issue: None,
            }),
            Err(error) => output.push(InventoryMcp {
                name: name.clone(),
                requirement: None,
                issue: Some(error.to_string()),
            }),
        }
    }
    Ok(output)
}

fn safe_mcp(name: &str, value: &serde_json::Value) -> Result<McpRequirement> {
    let object = value
        .as_object()
        .ok_or_else(|| AruError::msg("MCP entry must be an object"))?;
    if object.contains_key("disabled") {
        return Err(AruError::msg("disabled MCP entries are not importable"));
    }
    for key in [
        "cwd",
        "env",
        "headers",
        "httpHeaders",
        "oauth",
        "auth",
        "appId",
        "chatgptAppId",
    ] {
        if object.contains_key(key) {
            return Err(AruError::msg(format!(
                "MCP {name:?} uses unsupported {key} configuration"
            )));
        }
    }
    let kind = object
        .get("type")
        .or_else(|| object.get("transport"))
        .and_then(serde_json::Value::as_str);
    let command = object.get("command");
    let url = object.get("url").and_then(serde_json::Value::as_str);
    if command.is_some() || kind == Some("stdio") {
        reject_mcp_fields(object, &["type", "transport", "command", "args"])?;
        let command = command
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AruError::msg("stdio MCP requires a string command"))?;
        if command.is_empty()
            || command.contains(['/', '\\'])
            || command.split_whitespace().count() != 1
            || command.starts_with('.')
            || contains_placeholder(command)
        {
            return Err(AruError::msg(
                "stdio MCP command must be one bare executable token",
            ));
        }
        let args = match object.get("args") {
            None => Vec::new(),
            Some(value) => value
                .as_array()
                .ok_or_else(|| AruError::msg("stdio MCP args must be an array"))?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| AruError::msg("stdio MCP args must contain strings"))
                })
                .collect::<Result<Vec<_>>>()?,
        };
        if args.iter().any(|argument| unsafe_argument(argument)) {
            return Err(AruError::msg(
                "stdio MCP arguments contain a path or plugin placeholder",
            ));
        }
        let requirement = McpRequirement {
            command: Some(command.into()),
            args,
            transport: Some("stdio".into()),
            registry: None,
            server: None,
            version: None,
            package_registry: None,
            url: None,
            env_vars: Vec::new(),
            env_http_headers: BTreeMap::new(),
            bearer_token_env: None,
            targets: None,
        };
        requirement.validate(name)?;
        return Ok(requirement);
    }
    reject_mcp_fields(object, &["type", "transport", "url"])?;
    let transport = kind.unwrap_or("streamable-http");
    if transport != "streamable-http" && transport != "http" {
        return Err(AruError::msg(format!(
            "unsupported MCP transport {transport:?}; only stdio and streamable-http are importable"
        )));
    }
    let url = url.ok_or_else(|| AruError::msg("remote MCP requires url"))?;
    if contains_placeholder(url) {
        return Err(AruError::msg("remote MCP URL contains variable expansion"));
    }
    crate::manifest::validate_https_url(url, "plugin MCP URL")?;
    let requirement = McpRequirement {
        transport: Some("streamable-http".into()),
        url: Some(url.into()),
        registry: None,
        server: None,
        version: None,
        package_registry: None,
        command: None,
        args: Vec::new(),
        env_vars: Vec::new(),
        env_http_headers: BTreeMap::new(),
        bearer_token_env: None,
        targets: None,
    };
    requirement.validate(name)?;
    Ok(requirement)
}

fn reject_mcp_fields(
    object: &serde_json::Map<String, serde_json::Value>,
    allowed: &[&str],
) -> Result<()> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(AruError::msg(format!(
            "MCP entry field {field:?} cannot be represented without loss"
        )));
    }
    Ok(())
}

fn unsafe_argument(value: &str) -> bool {
    let windows_absolute = value.starts_with(['\\', '/'])
        || value
            .as_bytes()
            .get(1)
            .is_some_and(|separator| *separator == b':');
    contains_placeholder(value)
        || value.starts_with("./")
        || value.starts_with("../")
        || value.starts_with(".\\")
        || value.starts_with("..\\")
        || windows_absolute
        || Path::new(value).is_absolute()
}

fn contains_placeholder(value: &str) -> bool {
    value.contains("${")
}

fn discover_skills(root: &Path, relative: &str) -> Result<(Vec<InventorySkill>, Vec<String>)> {
    let relative = contained_relative(relative)?;
    let directory = root.join(&relative);
    if !directory.exists() {
        return Ok((Vec::new(), Vec::new()));
    }
    let metadata = std::fs::symlink_metadata(&directory).at(&directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok((
            Vec::new(),
            vec![format!("invalid skills location {relative:?}")],
        ));
    }
    let mut output = Vec::new();
    let mut diagnostics = Vec::new();
    let mut children = std::fs::read_dir(&directory)
        .at(&directory)?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| AruError::msg(format!("could not inspect skills: {error}")))?;
    children.sort_by_key(std::fs::DirEntry::file_name);
    for child in children {
        let path = child.path();
        let metadata = std::fs::symlink_metadata(&path).at(&path)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || !path.join("SKILL.md").is_file()
        {
            continue;
        }
        let expected = child
            .file_name()
            .to_str()
            .ok_or_else(|| AruError::msg("skill directory name is not UTF-8"))?
            .to_owned();
        match crate::skill::discover_candidates(&path, &expected, &BTreeMap::new()) {
            Ok(candidates) if candidates.len() == 1 && candidates[0].absolute_path == path => {
                let skill = &candidates[0];
                output.push(InventorySkill {
                    name: skill.name.clone(),
                    relative_path: format!("{relative}/{}", skill.name),
                    absolute_path: skill.absolute_path.clone(),
                    sha256: skill.sha256.clone(),
                });
            }
            Ok(_) => diagnostics.push(format!("skipped invalid skill {expected:?}")),
            Err(error) => diagnostics.push(format!("skipped invalid skill {expected:?}: {error}")),
        }
    }
    reject_duplicate_skills(&output)?;
    Ok((output, diagnostics))
}

fn reject_duplicate_skills(skills: &[InventorySkill]) -> Result<()> {
    let mut names = std::collections::BTreeSet::new();
    for skill in skills {
        if !names.insert(&skill.name) {
            return Err(AruError::msg(format!(
                "duplicate plugin skill name {:?}",
                skill.name
            )));
        }
    }
    Ok(())
}

fn declared_paths(value: Option<&serde_json::Value>, default: &str) -> Result<Vec<String>> {
    let Some(value) = value else {
        return Ok(vec![default.into()]);
    };
    if let Some(path) = value.as_str() {
        return Ok(vec![contained_relative(path)?]);
    }
    let paths = value
        .as_array()
        .ok_or_else(|| AruError::msg("OpenAI skills must be a path or path array"))?;
    paths
        .iter()
        .map(|value| {
            let path = value
                .as_str()
                .or_else(|| value.get("path").and_then(serde_json::Value::as_str))
                .ok_or_else(|| AruError::msg("OpenAI skill entry requires path"))?;
            contained_relative(path)
        })
        .collect()
}

fn contained_relative(value: &str) -> Result<String> {
    let value = value.strip_prefix("./").unwrap_or(value);
    let path = crate::skill::validate_relative_selector(value)?;
    if path.as_os_str().is_empty() {
        return Err(AruError::msg("plugin component path must not be empty"));
    }
    path.to_str()
        .map(|path| path.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| AruError::msg("plugin component path is not UTF-8"))
}

fn fixed_unsupported(root: &Path, entries: &[(&str, &str)]) -> Vec<String> {
    entries
        .iter()
        .filter(|(path, _)| root.join(path).exists())
        .map(|(_, capability)| (*capability).to_owned())
        .collect()
}

fn openai_unsupported(value: &serde_json::Value) -> Vec<String> {
    let Some(object) = value.as_object() else {
        return vec!["openai:invalid-overlay".into()];
    };
    ["apps", "hooks", "commands"]
        .into_iter()
        .filter(|key| active(object.get(*key)))
        .map(|key| format!("openai:{key}"))
        .chain(
            object
                .get("paths")
                .and_then(serde_json::Value::as_object)
                .into_iter()
                .flat_map(|paths| {
                    ["apps", "hooks"]
                        .into_iter()
                        .filter(|key| active(paths.get(*key)))
                })
                .map(|key| format!("openai:{key}")),
        )
        .collect()
}

fn active(value: Option<&serde_json::Value>) -> bool {
    value.is_some_and(|value| match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::String(value) => !value.is_empty(),
        serde_json::Value::Array(value) => !value.is_empty(),
        serde_json::Value::Object(value) => !value.is_empty(),
        serde_json::Value::Number(_) => true,
    })
}

fn portable_schema(root: &Path) -> Result<Option<String>> {
    let path = root.join("plugin.json");
    if !path.exists() {
        return Ok(None);
    }
    let value = read_json(&path)?;
    Ok(value
        .get("$schema")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned))
}

fn portable_extension(root: &Path, namespace: &str) -> Result<Option<serde_json::Value>> {
    let path = root.join("plugin.json");
    if !path.exists() {
        return Ok(None);
    }
    Ok(read_json(&path)?
        .get("extensions")
        .and_then(serde_json::Value::as_object)
        .and_then(|extensions| extensions.get(namespace))
        .cloned())
}

fn validate_portable_manifest_shape(value: &serde_json::Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| AruError::msg("plugin.json must contain a JSON object"))?;
    for field in [
        "version",
        "description",
        "homepage",
        "repository",
        "license",
    ] {
        if let Some(value) = object.get(field)
            && !value.is_string()
        {
            return Err(AruError::msg(format!(
                "plugin.json {field} must be a string"
            )));
        }
    }
    if let Some(author) = object.get("author") {
        let author = author
            .as_object()
            .ok_or_else(|| AruError::msg("plugin.json author must be an object"))?;
        for (field, value) in author {
            if !["name", "email", "url"].contains(&field.as_str()) || !value.is_string() {
                return Err(AruError::msg(format!(
                    "invalid plugin.json author field {field:?}"
                )));
            }
        }
    }
    if let Some(keywords) = object.get("keywords")
        && !keywords
            .as_array()
            .is_some_and(|values| values.iter().all(serde_json::Value::is_string))
    {
        return Err(AruError::msg("plugin.json keywords must be a string array"));
    }
    Ok(())
}

fn unknown_portable_fields(value: &serde_json::Value) -> Vec<String> {
    let allowed = [
        "$schema",
        "name",
        "version",
        "description",
        "author",
        "homepage",
        "repository",
        "license",
        "keywords",
        "extensions",
    ];
    value
        .as_object()
        .into_iter()
        .flat_map(|object| object.keys())
        .filter(|key| !allowed.contains(&key.as_str()))
        .map(|key| format!("ignored unknown plugin.json field {key:?}"))
        .collect()
}

fn validate_portable_name(name: &str) -> Result<()> {
    let bytes = name.as_bytes();
    let valid = (1..=64).contains(&bytes.len())
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-' || *byte == b'.'
        })
        && !name.contains("--")
        && !name.contains("..");
    if valid {
        Ok(())
    } else {
        Err(AruError::msg(format!(
            "invalid Agent Plugins name {name:?}"
        )))
    }
}

fn read_json(path: &Path) -> Result<serde_json::Value> {
    let metadata = std::fs::symlink_metadata(path).at(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AruError::msg(format!(
            "plugin manifest {} must be a regular non-symlink file",
            path.display()
        )));
    }
    if metadata.len() > MANIFEST_MAX_BYTES {
        return Err(AruError::msg(format!(
            "plugin manifest {} exceeds {MANIFEST_MAX_BYTES} bytes",
            path.display()
        )));
    }
    let bytes = std::fs::read(path).at(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| AruError::msg(format!("invalid JSON in {}: {error}", path.display())))
}

fn manifest_record(root: &Path, relative: &str) -> Result<ContributingManifest> {
    let path = root.join(relative);
    Ok(ContributingManifest {
        path: relative.replace(std::path::MAIN_SEPARATOR, "/"),
        sha256: crate::digest::sha256_bytes(&std::fs::read(&path).at(&path)?),
    })
}

#[cfg(test)]
mod tests;
