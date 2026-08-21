use serde::Serialize;

use crate::digest::canonical_json_digest;
use crate::error::{AruError, Result};
use crate::lockfile::{AruPackage, Lockfile, McpServer, PluginPackage, SkillPackage};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Bom {
    bom_format: &'static str,
    spec_version: &'static str,
    version: u32,
    metadata: Metadata,
    components: Vec<Component>,
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Serialize)]
struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<String>,
    component: RootComponent,
    properties: Vec<Property>,
}

#[derive(Debug, Serialize)]
struct RootComponent {
    #[serde(rename = "type")]
    component_type: &'static str,
    name: &'static str,
    #[serde(rename = "bom-ref")]
    bom_ref: &'static str,
}

#[derive(Debug, Serialize)]
struct Component {
    #[serde(rename = "type")]
    component_type: &'static str,
    #[serde(rename = "bom-ref")]
    bom_ref: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    hashes: Vec<ComponentHash>,
    #[serde(rename = "externalReferences", skip_serializing_if = "Vec::is_empty")]
    external_references: Vec<ExternalReference>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    properties: Vec<Property>,
}

#[derive(Debug, Serialize)]
struct ComponentHash {
    alg: &'static str,
    content: String,
}

#[derive(Debug, Serialize)]
struct ExternalReference {
    #[serde(rename = "type")]
    reference_type: &'static str,
    url: String,
}

#[derive(Debug, Serialize)]
struct Property {
    name: String,
    value: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Dependency {
    #[serde(rename = "ref")]
    reference: String,
    depends_on: Vec<String>,
}

pub fn cyclonedx_1_5(lock: &Lockfile, timestamp: Option<&str>) -> Result<Vec<u8>> {
    if let Some(timestamp) = timestamp {
        validate_timestamp(timestamp)?;
    }
    let mut components = Vec::new();
    let mut instruction_refs = std::collections::BTreeMap::new();
    let mut skill_refs = std::collections::BTreeMap::new();
    let mut mcp_refs = std::collections::BTreeMap::new();
    let mut package_refs = std::collections::BTreeMap::new();
    let mut plugin_refs = std::collections::BTreeMap::new();
    for source in &lock.instruction_sources {
        let bom_ref = reference(
            "instruction",
            &(source.source.as_str(), source.sha256.as_str()),
        )?;
        instruction_refs.insert(source.source.clone(), bom_ref.clone());
        components.push(Component {
            component_type: "data",
            bom_ref,
            name: source.source.clone(),
            version: None,
            hashes: vec![hash(&source.sha256)],
            external_references: Vec::new(),
            properties: sorted_properties([
                ("aru:kind", "instruction".into()),
                (
                    "aru:targets",
                    source
                        .targets
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                (
                    "aru:scope",
                    serde_json::to_string(&source.scope).map_err(|error| {
                        AruError::msg(format!("could not export instruction scope: {error}"))
                    })?,
                ),
            ]),
        });
    }
    for package in &lock.skill_packages {
        let component = skill_component(package)?;
        skill_refs.insert(package.source.clone(), component.bom_ref.clone());
        components.push(component);
    }
    for server in &lock.mcp_servers {
        let component = mcp_component(server)?;
        mcp_refs.insert(server.name.clone(), component.bom_ref.clone());
        components.push(component);
    }
    for package in &lock.aru_packages {
        let component = aru_package_component(package)?;
        package_refs.insert(package.source.clone(), component.bom_ref.clone());
        components.push(component);
    }
    for plugin in &lock.plugin_packages {
        let component = plugin_component(plugin)?;
        plugin_refs.insert(plugin.name.clone(), component.bom_ref.clone());
        components.push(component);
    }
    components.sort_by(|left, right| left.bom_ref.cmp(&right.bom_ref));
    let root_depends_on = components
        .iter()
        .map(|component| component.bom_ref.clone())
        .collect();
    let mut dependencies = vec![Dependency {
        reference: "aru:root".into(),
        depends_on: root_depends_on,
    }];
    for package in &lock.aru_packages {
        let mut depends_on = package
            .dependencies
            .iter()
            .filter_map(|source| package_refs.get(source).cloned())
            .chain(
                package
                    .instruction_sources
                    .iter()
                    .filter_map(|source| instruction_refs.get(&source.source).cloned()),
            )
            .chain(skill_refs.get(&package.source).cloned())
            .chain(
                package
                    .mcp
                    .iter()
                    .filter_map(|name| mcp_refs.get(name).cloned()),
            )
            .collect::<Vec<_>>();
        depends_on.sort();
        depends_on.dedup();
        dependencies.push(Dependency {
            reference: package_refs[&package.source].clone(),
            depends_on,
        });
    }
    for plugin in &lock.plugin_packages {
        let mut depends_on =
            lock.skill_packages
                .iter()
                .filter(|package| {
                    package.skills.iter().any(|skill| {
                        skill.origin.as_ref().is_some_and(|origin| {
                            origin.kind == "plugin" && origin.name == plugin.name
                        })
                    })
                })
                .filter_map(|package| skill_refs.get(&package.source).cloned())
                .chain(
                    plugin
                        .mcp
                        .iter()
                        .filter_map(|name| mcp_refs.get(name).cloned()),
                )
                .collect::<Vec<_>>();
        depends_on.sort();
        depends_on.dedup();
        dependencies.push(Dependency {
            reference: plugin_refs[&plugin.name].clone(),
            depends_on,
        });
    }
    dependencies[1..].sort_by(|left, right| left.reference.cmp(&right.reference));
    let bom = Bom {
        bom_format: "CycloneDX",
        spec_version: "1.5",
        version: 1,
        metadata: Metadata {
            timestamp: timestamp.map(str::to_owned),
            component: RootComponent {
                component_type: "application",
                name: "aru-project",
                bom_ref: "aru:root",
            },
            properties: vec![Property {
                name: "aru:document-purpose".into(),
                value: "inventory".into(),
            }],
        },
        components,
        dependencies,
    };
    let mut bytes = serde_json::to_vec_pretty(&bom).map_err(|error| {
        AruError::msg(format!("could not serialize CycloneDX inventory: {error}"))
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn validate_exportable(lock: &Lockfile) -> Result<()> {
    cyclonedx_1_5(lock, None).map(|_| ())
}

fn aru_package_component(package: &AruPackage) -> Result<Component> {
    let bom_ref = reference(
        "aru-package",
        &(
            package.source.as_str(),
            package.package_version.as_str(),
            package.revision.as_str(),
        ),
    )?;
    Ok(Component {
        component_type: "library",
        bom_ref,
        name: package.name.clone(),
        version: Some(package.package_version.clone()),
        hashes: vec![hash(&package.content_sha256)],
        external_references: vec![ExternalReference {
            reference_type: "vcs",
            url: scrub_url(&package.source, "aru package source URL")?,
        }],
        properties: sorted_properties([
            ("aru:kind", "aru-package".into()),
            ("aru:manifest-sha256", package.manifest_sha256.clone()),
            ("aru:revision", package.revision.clone()),
            ("aru:requirement", package.requirement.clone()),
            (
                "aru:targets",
                package
                    .targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ]),
    })
}

fn plugin_component(plugin: &PluginPackage) -> Result<Component> {
    let bom_ref = reference(
        "plugin-package",
        &(
            plugin.name.as_str(),
            plugin.source.as_str(),
            plugin.revision.as_str(),
            plugin.format,
        ),
    )?;
    Ok(Component {
        component_type: "library",
        bom_ref,
        name: plugin.name.clone(),
        version: plugin
            .declared_version
            .clone()
            .or_else(|| Some(plugin.version.clone())),
        hashes: vec![hash(&plugin.tree_sha256)],
        external_references: vec![ExternalReference {
            reference_type: "vcs",
            url: scrub_url(&plugin.source, "plugin source URL")?,
        }],
        properties: sorted_properties([
            ("aru:kind", "plugin-package".into()),
            ("aru:plugin-format", plugin.format.to_string()),
            (
                "aru:compatibility",
                plugin
                    .unsupported
                    .iter()
                    .chain(plugin.diagnostics.iter())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            ("aru:revision", plugin.revision.clone()),
            ("aru:requirement", plugin.requirement.clone()),
            (
                "aru:manifests",
                plugin
                    .manifests
                    .iter()
                    .map(|manifest| format!("{}={}", manifest.path, manifest.sha256))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            (
                "aru:selection",
                serde_json::to_string(&plugin.selection).map_err(|error| {
                    AruError::msg(format!("could not export plugin selection: {error}"))
                })?,
            ),
            (
                "aru:targets",
                plugin
                    .targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ]),
    })
}

fn skill_component(package: &SkillPackage) -> Result<Component> {
    let bom_ref = reference(
        "skill-package",
        &(
            package.source.as_str(),
            package.version.as_str(),
            package.revision.as_str(),
        ),
    )?;
    let source = package
        .skills
        .first()
        .and_then(|skill| skill.origin.as_ref())
        .map(|origin| origin.source.as_str())
        .unwrap_or(&package.source);
    let source = scrub_url(source, "skill source URL")?;
    Ok(Component {
        component_type: "library",
        bom_ref,
        name: package.repository_name.clone(),
        version: Some(package.version.clone()),
        hashes: Vec::new(),
        external_references: vec![ExternalReference {
            reference_type: "vcs",
            url: source,
        }],
        properties: sorted_properties([
            ("aru:kind", "skill-package".into()),
            ("aru:revision", package.revision.clone()),
            ("aru:requirement", package.requirement.clone()),
            (
                "aru:targets",
                package
                    .targets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            (
                "aru:exports",
                package
                    .skills
                    .iter()
                    .map(|skill| skill.name.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ]),
    })
}

fn mcp_component(server: &McpServer) -> Result<Component> {
    let bom_ref = reference(
        "mcp",
        &(
            server.name.as_str(),
            server.server_id.as_str(),
            server.version.as_str(),
            server.metadata_sha256.as_str(),
        ),
    )?;
    let mut external_references = Vec::new();
    if let Some(registry) = &server.registry {
        external_references.push(ExternalReference {
            reference_type: "distribution",
            url: scrub_url(registry, "MCP registry URL")?,
        });
    }
    let mut remote_urls = server
        .targets
        .iter()
        .filter_map(|target| target.url.as_deref())
        .map(|url| scrub_url(url, "MCP remote URL"))
        .collect::<Result<Vec<_>>>()?;
    remote_urls.sort();
    remote_urls.dedup();
    external_references.extend(remote_urls.into_iter().map(|url| ExternalReference {
        reference_type: "distribution",
        url,
    }));
    external_references.sort_by(|left, right| {
        (left.reference_type, &left.url).cmp(&(right.reference_type, &right.url))
    });
    let mut transports = server
        .targets
        .iter()
        .map(|target| target.transport.as_str())
        .collect::<Vec<_>>();
    transports.sort();
    transports.dedup();
    Ok(Component {
        component_type: "application",
        bom_ref,
        name: server.name.clone(),
        version: Some(server.version.clone()),
        hashes: vec![hash(&server.metadata_sha256)],
        external_references,
        properties: sorted_properties([
            ("aru:kind", "mcp".into()),
            ("aru:server-id", server.server_id.clone()),
            ("aru:requirement", server.requirement.clone()),
            ("aru:transports", transports.join(",")),
            (
                "aru:targets",
                server
                    .targets
                    .iter()
                    .map(|target| target.target.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ]),
    })
}

fn hash(value: &str) -> ComponentHash {
    ComponentHash {
        alg: "SHA-256",
        content: value.strip_prefix("sha256:").unwrap_or(value).into(),
    }
}

fn reference(prefix: &str, identity: &impl Serialize) -> Result<String> {
    let digest = canonical_json_digest(identity)?;
    Ok(format!(
        "aru:{prefix}:{}",
        digest.strip_prefix("sha256:").unwrap_or(&digest)
    ))
}

fn sorted_properties<const N: usize>(values: [(&str, String); N]) -> Vec<Property> {
    let mut properties = values
        .into_iter()
        .map(|(name, value)| Property {
            name: name.into(),
            value,
        })
        .collect::<Vec<_>>();
    properties.sort_by(|left, right| left.name.cmp(&right.name));
    properties
}

pub(crate) fn scrub_url(value: &str, kind: &str) -> Result<String> {
    let mut parsed = url::Url::parse(value)
        .map_err(|_| AruError::msg(format!("cannot export {kind} {value:?}: invalid URL")))?;
    if parsed.password().is_some() {
        parsed
            .set_password(None)
            .map_err(|_| AruError::msg(format!("cannot scrub credentials from {kind}")))?;
    }
    if matches!(parsed.scheme(), "http" | "https" | "git+http" | "git+https")
        && !parsed.username().is_empty()
    {
        parsed
            .set_username("")
            .map_err(|_| AruError::msg(format!("cannot scrub credentials from {kind}")))?;
    }
    Ok(parsed.to_string())
}

fn validate_timestamp(value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let digits = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
        .into_iter()
        .all(|index| bytes.get(index).is_some_and(u8::is_ascii_digit));
    let shape = bytes.len() == 20
        && digits
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'Z';
    let number = |start: usize, end: usize| {
        value[start..end]
            .parse::<u32>()
            .expect("timestamp digit positions were validated")
    };
    if !shape
        || !(1..=12).contains(&number(5, 7))
        || !(1..=31).contains(&number(8, 10))
        || number(11, 13) > 23
        || number(14, 16) > 59
        || number(17, 19) > 59
    {
        return Err(AruError::msg(
            "expected an RFC 3339 UTC timestamp like 2026-07-31T00:00:00Z",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_validation_is_strict_and_url_credentials_are_scrubbed() {
        assert!(validate_timestamp("2026-07-31T00:00:00Z").is_ok());
        for invalid in [
            "today",
            "2026-13-31T00:00:00Z",
            "2026-07-31T25:00:00Z",
            "2026-07-31T00:00:00+00:00",
        ] {
            assert!(validate_timestamp(invalid).is_err());
        }
        assert_eq!(
            scrub_url("https://user:secret@example.com/registry", "test URL").unwrap(),
            "https://example.com/registry"
        );
        assert_eq!(
            scrub_url("git+ssh://git@example.com/repo", "test URL").unwrap(),
            "git+ssh://git@example.com/repo"
        );
    }
}
