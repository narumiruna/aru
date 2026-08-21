use super::*;

fn skill(root: &Path, name: &str) {
    let directory = root.join("skills").join(name);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: demo\n---\nBody\n"),
    )
    .unwrap();
}

#[test]
fn detects_agent_plugin_and_accepts_safe_mcp() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("plugin.json"),
        format!(r#"{{"$schema":"{AGENT_PLUGIN_SCHEMA}","name":"demo"}}"#),
    )
    .unwrap();
    skill(temp.path(), "review");
    std::fs::write(
        temp.path().join("mcp.json"),
        format!(r#"{{"$schema":"{AGENT_MCP_SCHEMA}","mcpServers":{{"docs":{{"type":"streamable-http","url":"https://example.com/mcp"}}}}}}"#),
    )
    .unwrap();
    let inventory = inspect_plugin_root(temp.path(), None).unwrap();
    assert_eq!(inventory.format, PluginFormat::AgentPlugins);
    assert_eq!(inventory.skills[0].name, "review");
    assert!(inventory.mcp[0].requirement.is_some());
}

#[test]
fn agent_mcp_without_servers_is_disabled_without_spurious_inventory() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("plugin.json"),
        format!(r#"{{"$schema":"{AGENT_PLUGIN_SCHEMA}","name":"demo"}}"#),
    )
    .unwrap();
    std::fs::write(
        temp.path().join("mcp.json"),
        format!(r#"{{"$schema":"{AGENT_MCP_SCHEMA}"}}"#),
    )
    .unwrap();

    let inventory = inspect_plugin_root(temp.path(), None).unwrap();

    assert!(inventory.mcp.is_empty());
    assert_eq!(
        inventory.diagnostics,
        ["disabled invalid mcp.json: missing required mcpServers"]
    );
}

#[test]
fn composite_is_openai_and_inline_extension_wins() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("plugin.json"),
        format!(r#"{{"$schema":"{AGENT_PLUGIN_SCHEMA}","name":"demo","extensions":{{"com.openai":{{"hooks":{{"x":true}}}}}}}}"#),
    )
    .unwrap();
    std::fs::create_dir_all(temp.path().join(".codex-plugin")).unwrap();
    std::fs::create_dir_all(temp.path().join("commands")).unwrap();
    std::fs::write(
        temp.path().join(".codex-plugin/plugin.json"),
        r#"{"apps":{"legacy":true}}"#,
    )
    .unwrap();
    let inventory = inspect_plugin_root(temp.path(), None).unwrap();
    assert_eq!(inventory.format, PluginFormat::Openai);
    assert!(inventory.unsupported.contains(&"openai:hooks".into()));
    assert!(inventory.unsupported.contains(&"openai:commands".into()));
    assert!(!inventory.unsupported.contains(&"openai:apps".into()));
}

#[test]
fn legacy_openai_supplements_default_and_declared_locations() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join(".codex-plugin")).unwrap();
    std::fs::write(
        temp.path().join(".codex-plugin/plugin.json"),
        r#"{"name":"legacy","version":"1.0.0","skills":"./extra-skills","mcpServers":"./extra.mcp.json"}"#,
    )
    .unwrap();
    skill(temp.path(), "default-skill");
    let custom = temp.path().join("extra-skills/custom-skill");
    std::fs::create_dir_all(&custom).unwrap();
    std::fs::write(
        custom.join("SKILL.md"),
        "---\nname: custom-skill\ndescription: custom\n---\nBody\n",
    )
    .unwrap();
    std::fs::write(
        temp.path().join(".mcp.json"),
        r#"{"mcpServers":{"default-mcp":{"command":"helper"}}}"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("extra.mcp.json"),
        r#"{"mcpServers":{"extra-mcp":{"url":"https://example.com/mcp","type":"streamable-http"}}}"#,
    )
    .unwrap();
    let inventory = inspect_plugin_root(temp.path(), None).unwrap();
    assert_eq!(
        inventory
            .skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect::<Vec<_>>(),
        ["custom-skill", "default-skill"]
    );
    assert_eq!(
        inventory
            .mcp
            .iter()
            .map(|server| server.name.as_str())
            .collect::<Vec<_>>(),
        ["default-mcp", "extra-mcp"]
    );
}

#[test]
fn rejects_unsupported_agent_schema_and_accepts_explicit_legacy_openai() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("plugin.json"),
        r#"{"$schema":"https://agent-plugins.org/schemas/1.1.0/plugin.schema.json","name":"future"}"#,
    )
    .unwrap();
    assert!(
        detect_format(temp.path(), None)
            .unwrap_err()
            .to_string()
            .contains("1.0.0")
    );

    let legacy = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(legacy.path().join(".codex-plugin")).unwrap();
    std::fs::write(
        legacy.path().join(".codex-plugin/plugin.json"),
        r#"{"name":"legacy","version":"1.0.0"}"#,
    )
    .unwrap();
    assert_eq!(
        detect_format(legacy.path(), Some(PluginFormat::Openai)).unwrap(),
        PluginFormat::Openai
    );
}

#[test]
fn safe_mcp_boundary_preserves_bare_argv_and_rejects_lossy_fields() {
    let safe: serde_json::Value =
        serde_json::from_str(r#"{"type":"stdio","command":"helper","args":["--mode","review"]}"#)
            .unwrap();
    let requirement = safe_mcp("helper", &safe).unwrap();
    assert_eq!(requirement.command.as_deref(), Some("helper"));
    assert_eq!(requirement.args, ["--mode", "review"]);

    for unsafe_value in [
        r#"{"type":"stdio","command":"./helper"}"#,
        r#"{"type":"stdio","command":"helper","cwd":"./data"}"#,
        r#"{"type":"stdio","command":"helper","env":{"TOKEN":"literal"}}"#,
        r#"{"type":"stdio","command":"helper","args":["${PLUGIN_ROOT}/x"]}"#,
        r#"{"type":"stdio","command":"helper","unknown":true}"#,
        r#"{"type":"sse","url":"https://example.com/sse"}"#,
        r#"{"type":"streamable-http","url":"https://example.com/mcp","headers":{}}"#,
        r#"{"type":"streamable-http","url":"http://example.com/mcp"}"#,
    ] {
        let value: serde_json::Value = serde_json::from_str(unsafe_value).unwrap();
        assert!(safe_mcp("unsafe", &value).is_err(), "{unsafe_value}");
    }
}

#[test]
fn rejects_unsafe_stdio_and_ambiguous_gemini_combo() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("plugin.json"),
        format!(r#"{{"$schema":"{AGENT_PLUGIN_SCHEMA}","name":"demo"}}"#),
    )
    .unwrap();
    std::fs::write(temp.path().join("gemini-extension.json"), r#"{"name":"g"}"#).unwrap();
    assert!(detect_format(temp.path(), None).is_err());
    let entry: serde_json::Value =
        serde_json::from_str(r#"{"type":"stdio","command":"./server","args":[]}"#).unwrap();
    assert!(safe_mcp("server", &entry).is_err());
}
