use std::collections::{BTreeMap, BTreeSet};

use crate::lockfile::Lockfile;

pub(super) fn lock_diff_plan(previous: Option<&Lockfile>, next: &Lockfile) -> Vec<String> {
    let mut plan = Vec::new();
    let previous_packages = previous
        .into_iter()
        .flat_map(|lock| &lock.aru_packages)
        .map(|package| (package.source.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    for package in &next.aru_packages {
        match previous_packages.get(package.source.as_str()) {
            None => plan.push(format!(
                "lock aru package {} {}",
                package.name, package.package_version
            )),
            Some(previous)
                if previous.package_version != package.package_version
                    || previous.revision != package.revision =>
            {
                plan.push(format!(
                    "lock aru package {} {} -> {}",
                    package.name, previous.package_version, package.package_version
                ));
            }
            Some(previous) if *previous != package => {
                plan.push(format!("refresh aru package {} graph intent", package.name));
            }
            _ => {}
        }
    }
    let next_package_sources = next
        .aru_packages
        .iter()
        .map(|package| package.source.as_str())
        .collect::<BTreeSet<_>>();
    for package in previous_packages
        .values()
        .filter(|package| !next_package_sources.contains(package.source.as_str()))
    {
        plan.push(format!("unlock removed aru package {}", package.name));
    }
    let previous_plugins = previous
        .into_iter()
        .flat_map(|lock| &lock.plugin_packages)
        .map(|plugin| (plugin.name.as_str(), plugin))
        .collect::<BTreeMap<_, _>>();
    for plugin in &next.plugin_packages {
        match previous_plugins.get(plugin.name.as_str()) {
            None => plan.push(format!("lock plugin {} {}", plugin.name, plugin.version)),
            Some(previous)
                if previous.version != plugin.version || previous.revision != plugin.revision =>
            {
                plan.push(format!(
                    "lock plugin {} {} -> {}",
                    plugin.name, previous.version, plugin.version
                ));
            }
            Some(previous) if *previous != plugin => {
                plan.push(format!("refresh plugin {} intent", plugin.name));
            }
            _ => {}
        }
    }
    let next_plugin_names = next
        .plugin_packages
        .iter()
        .map(|plugin| plugin.name.as_str())
        .collect::<BTreeSet<_>>();
    for plugin in previous_plugins
        .values()
        .filter(|plugin| !next_plugin_names.contains(plugin.name.as_str()))
    {
        plan.push(format!("unlock removed plugin {}", plugin.name));
    }
    let previous_instructions: BTreeMap<_, _> = previous
        .into_iter()
        .flat_map(|lock| &lock.instruction_sources)
        .map(|source| (source.source.as_str(), source))
        .collect();
    for source in &next.instruction_sources {
        match previous_instructions.get(source.source.as_str()) {
            None => plan.push(format!("lock instruction {}", source.source)),
            Some(previous) if *previous != source && previous.sha256 == source.sha256 => {
                plan.push(format!(
                    "refresh instruction {} projection intent",
                    source.source
                ));
            }
            Some(previous) if *previous != source => {
                let _ = previous;
                plan.push(format!("lock instruction {}", source.source));
            }
            _ => {}
        }
    }
    let next_instruction_sources = next
        .instruction_sources
        .iter()
        .map(|source| source.source.as_str())
        .collect::<BTreeSet<_>>();
    for source in previous_instructions
        .keys()
        .filter(|source| !next_instruction_sources.contains(**source))
    {
        plan.push(format!("unlock removed instruction {source}"));
    }
    let previous_skills: BTreeMap<_, _> = previous
        .into_iter()
        .flat_map(|lock| &lock.skill_packages)
        .flat_map(|package| {
            package.skills.iter().map(move |skill| {
                (
                    skill.name.as_str(),
                    (package.version.as_str(), skill.sha256.as_str()),
                )
            })
        })
        .collect();
    for package in &next.skill_packages {
        for skill in &package.skills {
            match previous_skills.get(skill.name.as_str()) {
                None => plan.push(format!("lock skill {} {}", skill.name, package.version)),
                Some((version, digest))
                    if *version != package.version || *digest != skill.sha256 =>
                {
                    plan.push(format!(
                        "lock skill {} {} -> {}",
                        skill.name, version, package.version
                    ));
                }
                _ => {}
            }
        }
    }
    let next_names: BTreeSet<_> = next
        .skill_packages
        .iter()
        .flat_map(|package| package.skills.iter().map(|skill| skill.name.as_str()))
        .collect();
    for name in previous_skills
        .keys()
        .filter(|name| !next_names.contains(**name))
    {
        plan.push(format!("unlock removed skill {name}"));
    }
    let previous_mcp: BTreeMap<_, _> = previous
        .into_iter()
        .flat_map(|lock| &lock.mcp_servers)
        .map(|server| (server.name.as_str(), server.version.as_str()))
        .collect();
    for server in &next.mcp_servers {
        match previous_mcp.get(server.name.as_str()) {
            None => plan.push(format!("lock MCP {} {}", server.name, server.version)),
            Some(version) if *version != server.version => plan.push(format!(
                "lock MCP {} {} -> {}",
                server.name, version, server.version
            )),
            _ => {}
        }
    }
    plan
}

pub(super) fn update_previews(
    previous: Option<&Lockfile>,
    next: &Lockfile,
    update_skills: &BTreeSet<String>,
    update_mcp: &BTreeSet<String>,
    update_packages: &BTreeSet<String>,
) -> Vec<String> {
    let mut previews = Vec::new();
    for source in update_packages {
        if let Some(candidate) = next
            .plugin_packages
            .iter()
            .find(|plugin| plugin.name == *source)
        {
            let next_label = format!(
                "{}@{}",
                candidate.version,
                short_digest(&candidate.revision)
            );
            let previous = previous.and_then(|lock| {
                lock.plugin_packages
                    .iter()
                    .find(|plugin| plugin.name == *source)
            });
            let transition = match previous {
                Some(previous)
                    if previous.version == candidate.version
                        && previous.revision == candidate.revision =>
                {
                    format!("{next_label} (unchanged)")
                }
                Some(previous) => format!(
                    "{}@{} -> {next_label}",
                    previous.version,
                    short_digest(&previous.revision)
                ),
                None => format!("unlocked -> {next_label}"),
            };
            previews.push(format!("plugin {} {transition}", candidate.name));
            continue;
        }
        let Some(candidate) = next
            .aru_packages
            .iter()
            .find(|package| package.source == *source)
        else {
            continue;
        };
        let next_label = format!(
            "{}@{}",
            candidate.package_version,
            short_digest(&candidate.revision)
        );
        let previous = previous.and_then(|lock| {
            lock.aru_packages
                .iter()
                .find(|package| package.source == *source)
        });
        let transition = match previous {
            Some(previous)
                if previous.package_version == candidate.package_version
                    && previous.revision == candidate.revision =>
            {
                format!("{next_label} (unchanged)")
            }
            Some(previous) => format!(
                "{}@{} -> {next_label}",
                previous.package_version,
                short_digest(&previous.revision)
            ),
            None => format!("unlocked -> {next_label}"),
        };
        previews.push(format!("aru package {} {transition}", candidate.name));
    }
    for source in update_skills {
        let Some(candidate) = next
            .skill_packages
            .iter()
            .find(|package| package.source == *source)
        else {
            continue;
        };
        let next_label = format!(
            "{}@{}",
            candidate.version,
            short_digest(&candidate.revision)
        );
        let previous = previous.and_then(|lock| {
            lock.skill_packages
                .iter()
                .find(|package| package.source == *source)
        });
        let transition = match previous {
            Some(previous)
                if previous.version == candidate.version
                    && previous.revision == candidate.revision =>
            {
                format!("{next_label} (unchanged)")
            }
            Some(previous) => format!(
                "{}@{} -> {next_label}",
                previous.version,
                short_digest(&previous.revision)
            ),
            None => format!("unlocked -> {next_label}"),
        };
        previews.push(format!("skill {} {transition}", candidate.repository_name));
    }
    for name in update_mcp {
        let Some(candidate) = next.mcp_servers.iter().find(|server| server.name == *name) else {
            continue;
        };
        let previous =
            previous.and_then(|lock| lock.mcp_servers.iter().find(|server| server.name == *name));
        let transition = match previous {
            Some(previous)
                if previous.version == candidate.version
                    && previous.metadata_sha256 == candidate.metadata_sha256 =>
            {
                format!("{} (unchanged)", candidate.version)
            }
            Some(previous) if previous.version != candidate.version => {
                format!("{} -> {}", previous.version, candidate.version)
            }
            Some(previous) => format!(
                "{} metadata {} -> {}",
                candidate.version,
                short_digest(&previous.metadata_sha256),
                short_digest(&candidate.metadata_sha256)
            ),
            None => format!("unlocked -> {}", candidate.version),
        };
        previews.push(format!("MCP {} {transition}", candidate.name));
    }
    previews.sort();
    previews
}

fn short_digest(value: &str) -> &str {
    let value = value.strip_prefix("sha256:").unwrap_or(value);
    &value[..value.len().min(7)]
}

pub(super) fn lock_details(lock: &Lockfile) -> Vec<String> {
    let mut details = Vec::new();
    for package in &lock.aru_packages {
        details.push(format!(
            "aru package {} {} {} from {}",
            package.name, package.package_version, package.revision, package.source
        ));
    }
    for plugin in &lock.plugin_packages {
        details.push(format!(
            "plugin {} {} {} {} from {}",
            plugin.name, plugin.format, plugin.version, plugin.revision, plugin.source
        ));
    }
    for source in &lock.instruction_sources {
        details.push(format!("instruction {} {}", source.source, source.sha256));
    }
    for package in &lock.skill_packages {
        for skill in &package.skills {
            details.push(format!(
                "skill {} {} {} {} from {}",
                skill.name, package.version, package.revision, skill.sha256, package.source
            ));
        }
    }
    for server in &lock.mcp_servers {
        details.push(format!(
            "MCP {} {} {}",
            server.name, server.version, server.metadata_sha256
        ));
    }
    details.sort();
    details
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockfile::{LockedMcpPackage, McpServer, McpTarget};
    use crate::manifest::Target;

    fn pypi_lock(version: &str, metadata: &str) -> Lockfile {
        let mut lock = Lockfile::empty();
        lock.mcp_servers.push(McpServer {
            name: "weather".into(),
            origin: None,
            registry: Some(crate::registry::DEFAULT_REGISTRY.into()),
            server_id: "io.example/weather".into(),
            requirement: "sha256:requirement".into(),
            version: version.into(),
            metadata_sha256: metadata.into(),
            targets: vec![McpTarget {
                target: Target::Codex,
                kind: "package".into(),
                transport: "stdio".into(),
                command: Some("uvx".into()),
                args: vec![format!("weather-mcp@{version}")],
                env_vars: Vec::new(),
                env_http_headers: BTreeMap::new(),
                url: None,
                bearer_token_env: None,
                package: Some(LockedMcpPackage {
                    registry: "pypi".into(),
                    identifier: "weather-mcp".into(),
                    version: version.into(),
                }),
            }],
        });
        lock
    }

    #[test]
    fn pypi_update_preview_reports_unchanged_and_version_transitions() {
        let selected = BTreeSet::from(["weather".to_owned()]);
        let previous = pypi_lock("0.5.0", "sha256:old");
        let unchanged = pypi_lock("0.5.0", "sha256:old");
        assert_eq!(
            update_previews(
                Some(&previous),
                &unchanged,
                &BTreeSet::new(),
                &selected,
                &BTreeSet::new(),
            ),
            ["MCP weather 0.5.0 (unchanged)"]
        );

        let updated = pypi_lock("0.6.0", "sha256:new");
        assert_eq!(
            update_previews(
                Some(&previous),
                &updated,
                &BTreeSet::new(),
                &selected,
                &BTreeSet::new(),
            ),
            ["MCP weather 0.5.0 -> 0.6.0"]
        );
        assert_eq!(previous.mcp_servers[0].version, "0.5.0");
        assert_eq!(updated.mcp_servers[0].version, "0.6.0");
    }
}
