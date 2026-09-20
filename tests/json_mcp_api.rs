use std::collections::BTreeMap;

use aru::digest::canonical_json_digest;
use aru::lockfile::McpTarget;
use aru::manifest::Target;
use aru::target::{claude::ClaudeConfig, copilot::CopilotConfig, normalized_entry};
use serde_json::{Value, json};

fn target(target: Target, transport: &str) -> McpTarget {
    McpTarget {
        target,
        kind: "command".into(),
        transport: transport.into(),
        command: Some("demo".into()),
        args: vec!["--serve".into()],
        env_vars: vec!["DEMO_TOKEN".into()],
        env_http_headers: BTreeMap::from([("X-Demo".into(), "DEMO_HEADER".into())]),
        bearer_token_env: Some("DEMO_TOKEN".into()),
        url: Some("https://example.com/mcp".into()),
        package: None,
    }
}

// Exercise both public concrete APIs without relying on the internal dispatch enum.
macro_rules! config_contract {
    ($test:ident, $config:ty, $target:ident, $path:literal) => {
        #[test]
        fn $test() {
            let temporary = tempfile::tempdir().unwrap();
            let project = temporary.path();
            let path = project.join($path);
            let mut config = <$config>::load(project).unwrap();
            assert_eq!(config.bytes().unwrap(), b"{}\n");
            assert_eq!(config.digest("missing").unwrap(), None);
            config.remove("missing");
            assert_eq!(config.bytes().unwrap(), b"{}\n");

            let entry = target(Target::$target, "stdio");
            config.set("managed", &entry).unwrap();
            assert_eq!(config.digest("managed").unwrap(), Some(aru::target::entry_digest(&entry).unwrap()));
            assert!(!path.exists(), "adapters must not write files");
            config.remove("managed");
            assert_eq!(config.bytes().unwrap(), b"{\n  \"mcpServers\": {}\n}\n");

            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let original = b"{\n  \"custom\": {\n    \"keep\": true\n  },\n  \"mcpServers\": {\n    \"unmanaged\": {\n      \"command\": \"keep\"\n    }\n  }\n}\n";
            std::fs::write(&path, original).unwrap();
            let mut config = <$config>::load(project).unwrap();
            assert_eq!(config.bytes().unwrap(), original);
            assert_eq!(config.digest("unmanaged").unwrap(), Some(canonical_json_digest(&json!({"command":"keep"})).unwrap()));
            config.set("managed", &entry).unwrap();
            let mut expected: Value = serde_json::from_slice(original).unwrap();
            expected["mcpServers"]["managed"] = normalized_entry(&entry).unwrap();
            let mut bytes = serde_json::to_vec_pretty(&expected).unwrap();
            bytes.push(b'\n');
            assert_eq!(config.bytes().unwrap(), bytes);
            config.remove("managed");
            assert_eq!(config.clone().bytes().unwrap(), original);
            assert_eq!(std::fs::read(&path).unwrap(), original);

            for (input, message) in [
                ("[]", concat!($path, " root must be a JSON object")),
                ("{\"mcpServers\":null}", concat!($path, " mcpServers must be an object")),
                ("{\"mcpServers\":[]}", concat!($path, " mcpServers must be an object")),
            ] {
                std::fs::write(&path, input).unwrap();
                assert_eq!(<$config>::load(project).unwrap_err().to_string(), message);
                assert_eq!(std::fs::read_to_string(&path).unwrap(), input);
            }
            std::fs::write(&path, "{").unwrap();
            match <$config>::load(project).unwrap_err() {
                aru::AruError::Json { path: error_path, .. } => assert_eq!(error_path, path),
                error => panic!("unexpected error: {error}"),
            }

            // The public adapters historically accept any renderable target;
            // target/adapter matching is enforced by McpConfig, not these APIs.
            std::fs::remove_file(&path).unwrap();
            let mut config = <$config>::load(project).unwrap();
            config.set("other", &target(Target::Codex, "stdio")).unwrap();
            let before = config.bytes().unwrap();
            assert!(config.set("other", &target(Target::$target, "sse")).is_err());
            assert_eq!(config.bytes().unwrap(), before);
        }
    };
}

config_contract!(
    claude_public_config_contract,
    ClaudeConfig,
    Claude,
    ".mcp.json"
);
config_contract!(
    copilot_public_config_contract,
    CopilotConfig,
    Copilot,
    ".github/mcp.json"
);

#[test]
fn claude_and_copilot_transport_fields_remain_exact() {
    for agent in [Target::Claude, Target::Copilot] {
        for (transport, mut expected) in [
            (
                "stdio",
                json!({"type":"stdio", "command":"demo", "args":["--serve"], "env":{"DEMO_TOKEN":"${DEMO_TOKEN}"}}),
            ),
            (
                "streamable-http",
                json!({"type":"http", "url":"https://example.com/mcp", "headers":{"X-Demo":"${DEMO_HEADER}", "Authorization":"Bearer ${DEMO_TOKEN}"}}),
            ),
        ] {
            if agent == Target::Copilot {
                expected["tools"] = json!(["*"]);
            }
            let mut entry = target(agent, transport);
            assert_eq!(normalized_entry(&entry).unwrap(), expected);
            entry.env_vars.clear();
            entry.env_http_headers.clear();
            entry.bearer_token_env = None;
            expected.as_object_mut().unwrap().remove("env");
            expected.as_object_mut().unwrap().remove("headers");
            assert_eq!(normalized_entry(&entry).unwrap(), expected);
        }
        let mut entry = target(agent, "streamable-http");
        entry
            .env_http_headers
            .insert("Authorization".into(), "OTHER_TOKEN".into());
        assert_eq!(
            normalized_entry(&entry).unwrap()["headers"]["Authorization"],
            "Bearer ${DEMO_TOKEN}"
        );
        assert_eq!(
            normalized_entry(&target(agent, "sse"))
                .unwrap_err()
                .to_string(),
            format!("unsupported MCP transport \"sse\" for {agent}")
        );
    }
}
