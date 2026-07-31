pub mod archive;
pub mod resolver;

use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{AruError, IoContext, Result};
use crate::manifest::{Instructions, McpRequirement, PackageRequirement, Target, validate_name};

pub const PACKAGE_MANIFEST_FILE: &str = "aru-package.toml";
pub const MAX_GRAPH_DEPTH: usize = 16;
pub const MAX_GRAPH_NODES: usize = 128;
pub const MAX_GRAPH_EDGES: usize = 512;
pub const MAX_PACKAGE_DEPTH: usize = 32;
pub const MAX_PACKAGE_ENTRIES: usize = 20_000;
pub const MAX_GRAPH_ENTRIES: usize = 100_000;
pub const MAX_GRAPH_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_AUDITED_TEXT_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageMetadata {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub package: PackageMetadata,
    #[serde(default)]
    pub instructions: Instructions,
    #[serde(default)]
    pub skills: BTreeMap<String, String>,
    #[serde(default)]
    pub mcp: BTreeMap<String, McpRequirement>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, PackageRequirement>,
}

impl PackageManifest {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(PACKAGE_MANIFEST_FILE);
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(AruError::msg(format!(
                    "Git source has no {PACKAGE_MANIFEST_FILE}; use `aru skill add` for a raw skill repository"
                )));
            }
            Err(source) => {
                return Err(AruError::Io {
                    path: path.clone(),
                    source,
                });
            }
        };
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(AruError::msg(format!(
                "{PACKAGE_MANIFEST_FILE} must be a regular non-symlink file"
            )));
        }
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(AruError::msg(format!(
                "{PACKAGE_MANIFEST_FILE} exceeds {MAX_MANIFEST_BYTES} bytes"
            )));
        }
        let text = std::fs::read_to_string(&path).at(&path)?;
        if text.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(AruError::msg(format!(
                "{PACKAGE_MANIFEST_FILE} exceeds {MAX_MANIFEST_BYTES} bytes"
            )));
        }
        reject_unknown_fields(&text, &path)?;
        let manifest: Self = toml::from_str(&text).map_err(|source| AruError::Toml {
            path: path.clone(),
            source,
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        validate_name(&self.package.name, "aru package name")?;
        semver::Version::parse(&self.package.version).map_err(|error| {
            AruError::msg(format!(
                "invalid aru package version {:?}: {error}",
                self.package.version
            ))
        })?;
        let all_targets = all_targets();
        for source in &self.instructions.sources {
            source.validate(&all_targets)?;
        }
        for (name, path) in &self.skills {
            validate_name(name, "package skill name")?;
            let parsed = crate::skill::validate_relative_selector(path)?;
            if parsed.as_os_str().is_empty() {
                return Err(AruError::msg("package skill path must not be empty"));
            }
        }
        for (name, requirement) in &self.mcp {
            requirement.validate(name)?;
            requirement.validate_targets(name, &all_targets)?;
        }
        for (source, requirement) in &self.dependencies {
            requirement.validate(source, &all_targets)?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct TreeBudget {
    pub entries: usize,
    pub bytes: u64,
}

pub fn tree_digest(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    for item in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || entry
                    .path()
                    .strip_prefix(root)
                    .ok()
                    .and_then(|path| path.components().next())
                    .is_none_or(
                        |component| !matches!(component, Component::Normal(name) if name == ".git"),
                    )
        })
    {
        let item =
            item.map_err(|error| AruError::msg(format!("aru package digest failed: {error}")))?;
        if !item.file_type().is_file() {
            continue;
        }
        let relative = item
            .path()
            .strip_prefix(root)
            .map_err(|_| AruError::msg("aru package digest path escaped its checkout"))?;
        let path = portable_path(relative)?;
        let digest = crate::digest::sha256_bytes(&std::fs::read(item.path()).at(item.path())?);
        files.push((path, digest));
    }
    files.sort();
    crate::digest::canonical_json_digest(&files)
}

pub fn validate_tree(root: &Path, budget: &mut TreeBudget) -> Result<()> {
    let mut package_entries = 0_usize;
    let mut folded = BTreeMap::<String, String>::new();
    let walker = WalkDir::new(root)
        .follow_links(false)
        .max_depth(MAX_PACKAGE_DEPTH)
        .into_iter()
        .filter_entry(|entry| {
            entry.depth() == 0
                || entry
                    .path()
                    .strip_prefix(root)
                    .ok()
                    .and_then(|path| path.components().next())
                    .is_none_or(
                        |component| !matches!(component, Component::Normal(name) if name == ".git"),
                    )
        });
    for item in walker {
        let item =
            item.map_err(|error| AruError::msg(format!("aru package tree scan failed: {error}")))?;
        if item.depth() == 0 {
            continue;
        }
        if item.depth() == MAX_PACKAGE_DEPTH
            && item.file_type().is_dir()
            && std::fs::read_dir(item.path())
                .at(item.path())?
                .next()
                .is_some()
        {
            return Err(AruError::msg(format!(
                "aru package tree exceeds maximum depth {MAX_PACKAGE_DEPTH}"
            )));
        }
        package_entries += 1;
        budget.entries += 1;
        if package_entries > MAX_PACKAGE_ENTRIES {
            return Err(AruError::msg(format!(
                "aru package tree exceeds {MAX_PACKAGE_ENTRIES} entries"
            )));
        }
        if budget.entries > MAX_GRAPH_ENTRIES {
            return Err(AruError::msg(format!(
                "aru package graph exceeds {MAX_GRAPH_ENTRIES} tree entries"
            )));
        }
        let relative = item
            .path()
            .strip_prefix(root)
            .map_err(|_| AruError::msg("aru package path escaped its checkout"))?;
        let portable = portable_path(relative)?;
        validate_portable_path(&portable)?;
        let folded_path = portable.to_lowercase();
        if let Some(previous) = folded.insert(folded_path, portable.clone()) {
            return Err(AruError::msg(format!(
                "aru package paths {previous:?} and {portable:?} collide on case-insensitive filesystems"
            )));
        }
        let metadata = std::fs::symlink_metadata(item.path()).at(item.path())?;
        if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
            return Err(AruError::msg(format!(
                "aru package entry {portable:?} must be a regular file or directory"
            )));
        }
        if !metadata.is_file() {
            continue;
        }
        budget.bytes = budget.bytes.saturating_add(metadata.len());
        if budget.bytes > MAX_GRAPH_BYTES {
            return Err(AruError::msg(format!(
                "aru package graph exceeds {MAX_GRAPH_BYTES} bytes"
            )));
        }
        if metadata.len() <= MAX_AUDITED_TEXT_BYTES {
            let bytes = std::fs::read(item.path()).at(item.path())?;
            if let Ok(content) = std::str::from_utf8(&bytes)
                && let Some((line, column, character)) = crate::audit::first_hidden_unicode(content)
            {
                return Err(AruError::msg(format!(
                    "aru package entry {portable:?}:{line}:{column} contains hidden Unicode U+{:04X}",
                    character as u32
                )));
            }
        }
    }
    Ok(())
}

fn reject_unknown_fields(text: &str, path: &Path) -> Result<()> {
    let value: toml::Value = toml::from_str(text).map_err(|source| AruError::Toml {
        path: path.to_path_buf(),
        source,
    })?;
    let root = value
        .as_table()
        .ok_or_else(|| AruError::msg("aru-package.toml must contain TOML tables"))?;
    reject_keys(
        root,
        &["package", "instructions", "skills", "mcp", "dependencies"],
        "root",
    )?;
    if let Some(package) = root.get("package").and_then(toml::Value::as_table) {
        reject_keys(package, &["name", "version"], "package")?;
    }
    if let Some(instructions) = root.get("instructions").and_then(toml::Value::as_table) {
        reject_keys(instructions, &["sources"], "instructions")?;
        if let Some(sources) = instructions.get("sources").and_then(toml::Value::as_array) {
            for source in sources {
                let table = source.as_table().ok_or_else(|| {
                    AruError::msg("instructions.sources entries must be TOML tables")
                })?;
                reject_keys(
                    table,
                    &["files", "exclude", "scope", "apply-to", "targets"],
                    "instructions.sources",
                )?;
            }
        }
    }
    if let Some(mcp) = root.get("mcp").and_then(toml::Value::as_table) {
        for (name, value) in mcp {
            let table = value
                .as_table()
                .ok_or_else(|| AruError::msg(format!("mcp.{name} must be a TOML table")))?;
            reject_keys(
                table,
                &[
                    "registry",
                    "server",
                    "version",
                    "transport",
                    "package-registry",
                    "url",
                    "command",
                    "args",
                    "bearer-token-env",
                    "targets",
                ],
                &format!("mcp.{name}"),
            )?;
        }
    }
    if let Some(dependencies) = root.get("dependencies").and_then(toml::Value::as_table) {
        for (source, value) in dependencies {
            let table = value.as_table().ok_or_else(|| {
                AruError::msg(format!(
                    "dependency {source:?} must be an inline TOML table"
                ))
            })?;
            reject_keys(
                table,
                &["version", "branch", "rev", "targets"],
                &format!("dependencies.{source}"),
            )?;
        }
    }
    Ok(())
}

fn reject_keys(
    table: &toml::map::Map<String, toml::Value>,
    allowed: &[&str],
    context: &str,
) -> Result<()> {
    for key in table.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(AruError::msg(format!(
                "unknown aru package field {context}.{key}"
            )));
        }
    }
    Ok(())
}

fn all_targets() -> Vec<Target> {
    vec![
        Target::Codex,
        Target::Claude,
        Target::Copilot,
        Target::Opencode,
        Target::Pi,
    ]
}

pub(crate) fn portable_path(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| AruError::msg("aru package path is not UTF-8"))
}

pub(crate) fn validate_portable_path(path: &str) -> Result<()> {
    for component in path.split('/') {
        let upper = component.trim_end_matches('.').to_ascii_uppercase();
        let stem = upper.split('.').next().unwrap_or(&upper);
        let reserved = matches!(stem, "CON" | "PRN" | "AUX" | "NUL")
            || stem
                .strip_prefix("COM")
                .or_else(|| stem.strip_prefix("LPT"))
                .is_some_and(|suffix| {
                    matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
                });
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with([' ', '.'])
            || component.contains(['<', '>', ':', '"', '|', '?', '*'])
            || component.chars().any(char::is_control)
            || reserved
        {
            return Err(AruError::msg(format!(
                "aru package path {path:?} is not portable"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
