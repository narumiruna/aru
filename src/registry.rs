use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::time::Duration;

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::digest::canonical_json_digest;
use crate::error::{AruError, Result};
use crate::lockfile::LockedMcpPackage;
use crate::manifest::{McpRequirement, Target, validate_https_url};

pub const DEFAULT_REGISTRY: &str = "https://registry.modelcontextprotocol.io";
pub const MAX_RESPONSE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_PAGES: usize = 100;
pub const MAX_VERSION_RECORDS: usize = 10_000;
const PATH_SEGMENT: &AsciiSet = &CONTROLS.add(b'/').add(b'?').add(b'#').add(b'%').add(b' ');

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolvedCandidate {
    pub kind: String,
    pub transport: String,
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env_vars: Vec<String>,
    pub env_http_headers: BTreeMap<String, String>,
    pub bearer_token_env: Option<String>,
    pub url: Option<String>,
    pub package: Option<LockedMcpPackage>,
}

#[derive(Debug, Clone)]
pub struct RegistryResolution {
    pub version: String,
    pub metadata_sha256: String,
    pub candidate: ResolvedCandidate,
}

#[derive(Debug, Deserialize)]
struct ServerList {
    servers: Vec<ServerResponse>,
    #[serde(default)]
    metadata: PageMetadata,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageMetadata {
    next_cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerResponse {
    server: ServerDetail,
    #[serde(default, rename = "_meta")]
    metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerDetail {
    name: String,
    version: String,
    #[serde(default)]
    packages: Vec<RegistryPackage>,
    #[serde(default)]
    remotes: Vec<RegistryRemote>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryPackage {
    registry_type: String,
    identifier: String,
    version: Option<String>,
    runtime_hint: Option<String>,
    transport: Transport,
    #[serde(default)]
    runtime_arguments: Vec<InputArgument>,
    #[serde(default)]
    package_arguments: Vec<InputArgument>,
    #[serde(default)]
    environment_variables: Vec<EnvironmentInput>,
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryRemote {
    #[serde(flatten)]
    transport: Transport,
    #[serde(default)]
    variables: BTreeMap<String, InputValue>,
}

#[derive(Debug, Clone, Deserialize)]
struct Transport {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    headers: Vec<EnvironmentInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputArgument {
    #[serde(rename = "type")]
    kind: String,
    name: Option<String>,
    value: Option<String>,
    default: Option<String>,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    is_secret: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentInput {
    name: String,
    value: Option<String>,
    default: Option<String>,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    is_secret: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputValue {
    value: Option<String>,
    default: Option<String>,
    #[serde(default)]
    is_required: bool,
    #[serde(default)]
    is_secret: bool,
}

pub struct RegistryClient {
    client: reqwest::blocking::Client,
}

impl RegistryClient {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 {
                    attempt.error("registry redirect limit exceeded")
                } else if attempt.url().scheme() != "https"
                    || !attempt.url().username().is_empty()
                    || attempt.url().password().is_some()
                {
                    attempt.error("registry redirect must remain credential-free HTTPS")
                } else {
                    attempt.follow()
                }
            }))
            .user_agent(concat!("aru/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self { client })
    }

    pub fn resolve(
        &self,
        requirement: &McpRequirement,
        targets: &[Target],
    ) -> Result<RegistryResolution> {
        let registry = requirement.registry.as_deref().unwrap_or(DEFAULT_REGISTRY);
        validate_https_url(registry, "registry URL")?;
        let server_id = requirement
            .server
            .as_deref()
            .ok_or_else(|| AruError::msg("registry MCP requirement has no server id"))?;
        let version_requirement = requirement.version.as_deref().unwrap_or("*");
        let response = if let Some(exact) = exact_literal(version_requirement) {
            self.fetch_exact(registry, server_id, exact)?
        } else {
            self.fetch_matching(registry, server_id, version_requirement)?
        };
        if response.server.name != server_id {
            return Err(AruError::msg("registry returned a different server id"));
        }
        if status(&response.metadata).is_some_and(|status| status != "active") {
            return Err(AruError::msg(format!(
                "registry server version {} is not active",
                response.server.version
            )));
        }
        let candidates = candidates(&response.server, requirement, targets)?;
        if candidates.len() != 1 {
            let options = candidates
                .iter()
                .map(|candidate| {
                    if let Some(package) = &candidate.package {
                        format!("{}/{}", candidate.transport, package.registry)
                    } else {
                        candidate.transport.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            return Err(AruError::msg(if candidates.is_empty() {
                "no MCP candidate satisfies the selectors and all target capabilities".to_owned()
            } else {
                format!(
                    "MCP candidate is ambiguous; choose --transport and/or --package-registry from: {options}"
                )
            }));
        }
        let candidate = candidates.into_iter().next().unwrap();
        let metadata_sha256 = canonical_json_digest(&serde_json::json!({
            "server": response.server.name,
            "version": response.server.version,
            "candidate": candidate,
        }))?;
        Ok(RegistryResolution {
            version: response.server.version,
            metadata_sha256,
            candidate,
        })
    }

    fn fetch_exact(&self, registry: &str, server: &str, version: &str) -> Result<ServerResponse> {
        let url = format!(
            "{}/v0.1/servers/{}/versions/{}",
            registry.trim_end_matches('/'),
            encode(server),
            encode(version)
        );
        self.get_json(&url)
    }

    fn fetch_matching(
        &self,
        registry: &str,
        server: &str,
        requirement: &str,
    ) -> Result<ServerResponse> {
        let requirement = VersionReq::parse(requirement).map_err(|error| {
            AruError::msg(format!(
                "invalid MCP SemVer requirement {requirement:?}: {error}"
            ))
        })?;
        let base = format!(
            "{}/v0.1/servers/{}/versions",
            registry.trim_end_matches('/'),
            encode(server)
        );
        let mut cursor: Option<String> = None;
        let mut seen = BTreeSet::new();
        let mut records = Vec::new();
        for _ in 0..MAX_PAGES {
            let url = if let Some(cursor) = &cursor {
                format!("{base}?cursor={}", encode(cursor))
            } else {
                base.clone()
            };
            let page: ServerList = self.get_json(&url)?;
            records.extend(page.servers);
            if records.len() > MAX_VERSION_RECORDS {
                return Err(AruError::msg(format!(
                    "registry exceeded version record limit {MAX_VERSION_RECORDS}"
                )));
            }
            cursor = checked_next_cursor(&mut seen, page.metadata.next_cursor)?;
            if cursor.is_none() {
                break;
            }
        }
        if cursor.is_some() {
            return Err(AruError::msg(format!(
                "registry exceeded pagination limit {MAX_PAGES}"
            )));
        }
        records
            .into_iter()
            .filter(|response| {
                response.server.name == server
                    && status(&response.metadata).is_none_or(|status| status == "active")
            })
            .filter_map(|response| {
                Version::parse(&response.server.version)
                    .ok()
                    .filter(|version| requirement.matches(version))
                    .map(|version| (version, response))
            })
            .max_by(|left, right| left.0.cmp(&right.0))
            .map(|(_, response)| response)
            .ok_or_else(|| {
                AruError::msg(format!(
                    "registry has no active SemVer version matching {requirement}"
                ))
            })
    }

    fn get_json<T: for<'de> Deserialize<'de>>(&self, url: &str) -> Result<T> {
        let mut response = self.client.get(url).send()?;
        if !response.status().is_success() {
            return Err(AruError::msg(format!(
                "registry request failed with HTTP {}",
                response.status()
            )));
        }
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| AruError::msg(format!("could not read registry response: {error}")))?;
        decode_response(&bytes)
    }
}

fn checked_next_cursor(
    seen: &mut BTreeSet<String>,
    next: Option<String>,
) -> Result<Option<String>> {
    let next = next.filter(|value| !value.is_empty());
    if let Some(next) = &next
        && !seen.insert(next.clone())
    {
        return Err(AruError::msg("registry pagination cursor cycle detected"));
    }
    Ok(next)
}

fn decode_response<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T> {
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(AruError::msg(format!(
            "registry response exceeds body limit {MAX_RESPONSE_BYTES} bytes"
        )));
    }
    serde_json::from_slice(bytes)
        .map_err(|error| AruError::msg(format!("registry JSON/schema decode failed: {error}")))
}

fn candidates(
    server: &ServerDetail,
    requirement: &McpRequirement,
    targets: &[Target],
) -> Result<Vec<ResolvedCandidate>> {
    let mut output = Vec::new();
    for package in &server.packages {
        if requirement
            .transport
            .as_ref()
            .is_some_and(|selected| selected != &package.transport.kind)
            || requirement
                .package_registry
                .as_ref()
                .is_some_and(|selected| selected != &package.registry_type)
        {
            continue;
        }
        if let Ok(candidate) = package_candidate(package, &server.version)
            && targets.iter().all(|target| supports(target, &candidate))
        {
            output.push(candidate);
        }
    }
    for remote in &server.remotes {
        if requirement
            .transport
            .as_ref()
            .is_some_and(|selected| selected != &remote.transport.kind)
            || requirement.package_registry.is_some()
        {
            continue;
        }
        if let Ok(candidate) = remote_candidate(remote)
            && targets.iter().all(|target| supports(target, &candidate))
        {
            output.push(candidate);
        }
    }
    output.sort_by(|left, right| {
        serde_json::to_string(left)
            .unwrap()
            .cmp(&serde_json::to_string(right).unwrap())
    });
    output.dedup();
    Ok(output)
}

fn package_candidate(package: &RegistryPackage, server_version: &str) -> Result<ResolvedCandidate> {
    if package.transport.kind != "stdio" || package.registry_type != "npm" {
        return Err(AruError::msg("unsupported registry package runtime"));
    }
    let mut env_vars = Vec::new();
    for input in &package.environment_variables {
        if input.is_secret || (input.value.is_none() && input.default.is_none()) {
            crate::manifest::validate_env_name(&input.name)?;
            env_vars.push(input.name.clone());
        } else if input.is_required {
            return Err(AruError::msg(
                "fixed registry package environment values are not supported by the portable lock model",
            ));
        }
    }
    env_vars.sort();
    env_vars.dedup();
    let runtime = package.runtime_hint.as_deref().unwrap_or("npx");
    if runtime != "npx" {
        return Err(AruError::msg(
            "MVP supports only the npm/npx package runtime",
        ));
    }
    let version = package.version.as_deref().unwrap_or(server_version);
    let mut args = Vec::new();
    for input in &package.runtime_arguments {
        append_argument(&mut args, input)?;
    }
    if !args.iter().any(|arg| arg == "--yes" || arg == "-y") {
        args.push("--yes".into());
    }
    args.push(format!("{}@{version}", package.identifier));
    for input in &package.package_arguments {
        append_argument(&mut args, input)?;
    }
    Ok(ResolvedCandidate {
        kind: "package".into(),
        transport: "stdio".into(),
        command: Some("npx".into()),
        args,
        env_vars,
        env_http_headers: BTreeMap::new(),
        bearer_token_env: None,
        url: None,
        package: Some(LockedMcpPackage {
            registry: "npm".into(),
            identifier: package.identifier.clone(),
            version: version.to_owned(),
        }),
    })
}

fn remote_candidate(remote: &RegistryRemote) -> Result<ResolvedCandidate> {
    if remote.transport.kind != "streamable-http" {
        return Err(AruError::msg("unsupported remote transport"));
    }
    let mut bearer_token_env = None;
    let mut env_http_headers = BTreeMap::new();
    for header in &remote.transport.headers {
        let template = header
            .value
            .as_ref()
            .or(header.default.as_ref())
            .ok_or_else(|| {
                AruError::msg("registry remote header has no portable value template")
            })?;
        let (prefix, variable) = template_variable(template).ok_or_else(|| {
            AruError::msg("registry remote header template is not a single environment reference")
        })?;
        let input = remote.variables.get(variable).ok_or_else(|| {
            AruError::msg("registry remote header references an undeclared variable")
        })?;
        if !input.is_secret || input.value.is_some() || input.default.is_some() {
            return Err(AruError::msg(
                "registry remote authentication must use an unresolved secret environment reference",
            ));
        }
        crate::manifest::validate_env_name(variable)?;
        if header.name.eq_ignore_ascii_case("authorization") && prefix == "Bearer " {
            bearer_token_env = Some(variable.to_owned());
        } else if prefix.is_empty() {
            env_http_headers.insert(header.name.clone(), variable.to_owned());
        } else {
            return Err(AruError::msg(
                "header prefix cannot be represented safely by every target",
            ));
        }
    }
    if remote
        .variables
        .values()
        .any(|input| input.is_required && !input.is_secret)
    {
        return Err(AruError::msg(
            "required non-secret remote variables are unsupported",
        ));
    }
    let url = remote
        .transport
        .url
        .as_deref()
        .ok_or_else(|| AruError::msg("remote transport has no URL"))?;
    validate_https_url(url, "registry MCP remote URL")?;
    Ok(ResolvedCandidate {
        kind: "remote".into(),
        transport: "streamable-http".into(),
        command: None,
        args: Vec::new(),
        env_vars: Vec::new(),
        env_http_headers,
        bearer_token_env,
        url: Some(url.into()),
        package: None,
    })
}

fn template_variable(template: &str) -> Option<(&str, &str)> {
    let open = template.find('{')?;
    let close = template[open + 1..].find('}')? + open + 1;
    if close + 1 != template.len() {
        return None;
    }
    let variable = &template[open + 1..close];
    (!variable.is_empty()).then_some((&template[..open], variable))
}

fn append_argument(output: &mut Vec<String>, input: &InputArgument) -> Result<()> {
    if input.is_secret {
        return Err(AruError::msg("secret registry arguments are unsupported"));
    }
    let value = input.value.as_ref().or(input.default.as_ref());
    if value.is_none() && input.is_required {
        return Err(AruError::msg(
            "required registry argument has no fixed value",
        ));
    }
    let Some(value) = value else {
        return Ok(());
    };
    match input.kind.as_str() {
        "positional" => output.push(value.clone()),
        "named" => {
            let name = input
                .name
                .as_deref()
                .ok_or_else(|| AruError::msg("named registry argument has no name"))?;
            if name.ends_with('=') {
                output.push(format!("{name}{value}"));
            } else {
                output.push(name.to_owned());
                output.push(value.clone());
            }
        }
        _ => return Err(AruError::msg("unknown registry argument type")),
    }
    Ok(())
}

pub fn supports(target: &Target, candidate: &ResolvedCandidate) -> bool {
    crate::target::supports_mcp_candidate(
        *target,
        &candidate.transport,
        candidate.command.is_some(),
        candidate.url.is_some(),
    )
}

fn status(metadata: &serde_json::Value) -> Option<&str> {
    metadata
        .get("io.modelcontextprotocol.registry/official")?
        .get("status")?
        .as_str()
}

fn exact_literal(requirement: &str) -> Option<&str> {
    requirement
        .strip_prefix('=')
        .filter(|value| !value.is_empty())
}

fn encode(value: &str) -> String {
    utf8_percent_encode(value, PATH_SEGMENT).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_selection_is_ambiguous_without_selector() {
        let server: ServerDetail = serde_json::from_value(serde_json::json!({
            "name": "io.example/test",
            "version": "1.0.0",
            "packages": [
                {"registryType":"npm","identifier":"one","version":"1.0.0","transport":{"type":"stdio"}},
                {"registryType":"npm","identifier":"two","version":"1.0.0","transport":{"type":"stdio"}}
            ]
        }))
        .unwrap();
        let requirement = McpRequirement {
            registry: Some(DEFAULT_REGISTRY.into()),
            server: Some("io.example/test".into()),
            version: None,
            transport: None,
            package_registry: None,
            url: None,
            command: None,
            args: Vec::new(),
            env_vars: Vec::new(),
            env_http_headers: BTreeMap::new(),
            bearer_token_env: None,
            targets: None,
        };
        let found = candidates(&server, &requirement, &[Target::Codex]).unwrap();
        assert_eq!(found.len(), 2);
        assert!(
            found[0].package.as_ref().unwrap().identifier
                < found[1].package.as_ref().unwrap().identifier
        );
    }

    #[test]
    fn unsupported_transport_and_auth_template_produce_no_candidate() {
        let server: ServerDetail = serde_json::from_value(serde_json::json!({
            "name": "io.example/unsupported",
            "version": "1.0.0",
            "remotes": [
                {"type":"sse","url":"https://example.com/sse"},
                {
                    "type":"streamable-http",
                    "url":"https://example.com/mcp",
                    "headers":[{"name":"Authorization","value":"Token prefix-{TOKEN}"}],
                    "variables":{"TOKEN":{"isRequired":true,"isSecret":true}}
                }
            ]
        }))
        .unwrap();
        let requirement = McpRequirement {
            registry: Some(DEFAULT_REGISTRY.into()),
            server: Some(server.name.clone()),
            version: None,
            transport: None,
            package_registry: None,
            url: None,
            command: None,
            args: Vec::new(),
            env_vars: Vec::new(),
            env_http_headers: BTreeMap::new(),
            bearer_token_env: None,
            targets: None,
        };
        assert!(
            candidates(&server, &requirement, &[Target::Codex, Target::Claude])
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn non_semver_requires_exact_literal() {
        assert_eq!(exact_literal("=release-2026"), Some("release-2026"));
        assert!(VersionReq::parse("release-2026").is_err());
    }

    #[test]
    fn local_openapi_fixtures_decode_portable_secret_references() {
        let first: ServerList =
            serde_json::from_str(include_str!("../tests/fixtures/registry/page-1.json")).unwrap();
        assert_eq!(first.metadata.next_cursor.as_deref(), Some("opaque:page/2"));
        let package = package_candidate(&first.servers[0].server.packages[0], "1.0.0").unwrap();
        assert_eq!(package.env_vars, vec!["EXAMPLE_TOKEN"]);
        assert_eq!(package.command.as_deref(), Some("npx"));

        let second: ServerList =
            serde_json::from_str(include_str!("../tests/fixtures/registry/page-2.json")).unwrap();
        let remote = remote_candidate(&second.servers[0].server.remotes[0]).unwrap();
        assert_eq!(remote.bearer_token_env.as_deref(), Some("EXAMPLE_TOKEN"));
        assert_eq!(remote.transport, "streamable-http");
        assert_eq!(status(&second.servers[1].metadata), Some("deprecated"));
    }

    #[test]
    fn fixture_candidate_order_does_not_choose_an_ambiguous_first_item() {
        let response: ServerResponse =
            serde_json::from_str(include_str!("../tests/fixtures/registry/ambiguous.json"))
                .unwrap();
        let requirement = McpRequirement {
            registry: Some(DEFAULT_REGISTRY.into()),
            server: Some(response.server.name.clone()),
            version: None,
            transport: None,
            package_registry: None,
            url: None,
            command: None,
            args: Vec::new(),
            env_vars: Vec::new(),
            env_http_headers: BTreeMap::new(),
            bearer_token_env: None,
            targets: None,
        };
        let forward = candidates(&response.server, &requirement, &[Target::Codex]).unwrap();
        let mut reversed_server = response.server;
        reversed_server.packages.reverse();
        let reversed = candidates(&reversed_server, &requirement, &[Target::Codex]).unwrap();
        assert_eq!(forward, reversed);
        assert_eq!(forward.len(), 2);
    }

    #[test]
    fn malformed_oversized_and_cyclic_registry_data_are_rejected() {
        assert!(decode_response::<ServerList>(b"not json").is_err());
        let oversized = vec![b' '; (MAX_RESPONSE_BYTES + 1) as usize];
        assert!(decode_response::<ServerList>(&oversized).is_err());
        let mut seen = BTreeSet::new();
        assert_eq!(
            checked_next_cursor(&mut seen, Some("opaque".into())).unwrap(),
            Some("opaque".into())
        );
        assert!(checked_next_cursor(&mut seen, Some("opaque".into())).is_err());
    }
}
