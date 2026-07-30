use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::error::{AruError, IoContext, Result};

pub const MANIFEST_FILE: &str = "aru.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    Codex,
    Claude,
    Copilot,
    Opencode,
    Pi,
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Codex => write!(f, "codex"),
            Self::Claude => write!(f, "claude"),
            Self::Copilot => write!(f, "copilot"),
            Self::Pi => write!(f, "pi"),
            Self::Opencode => write!(f, "opencode"),
        }
    }
}

impl std::str::FromStr for Target {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "codex" => Ok(Self::Codex),
            "claude" => Ok(Self::Claude),
            "copilot" => Ok(Self::Copilot),
            "pi" => Ok(Self::Pi),
            "opencode" => Ok(Self::Opencode),
            _ => Err(format!(
                "unknown target {value:?}; expected codex, claude, copilot, opencode, or pi"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    #[serde(default)]
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InstructionSourceScope {
    SourceDirectory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct InstructionSource {
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<InstructionSourceScope>,
    #[serde(default, rename = "apply-to", skip_serializing_if = "Vec::is_empty")]
    pub apply_to: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<Target>,
}

impl InstructionSource {
    pub fn validate(&self, project_targets: &[Target]) -> Result<()> {
        if self.files.is_empty() {
            return Err(AruError::msg("instruction source files must not be empty"));
        }
        if self.scope.is_some() == !self.apply_to.is_empty() {
            return Err(AruError::msg(
                "instruction source must set exactly one of scope or apply-to",
            ));
        }
        for pattern in self
            .files
            .iter()
            .chain(self.exclude.iter())
            .chain(self.apply_to.iter())
        {
            validate_portable_pattern(pattern)?;
        }
        let unique: BTreeSet<_> = self.targets.iter().collect();
        if unique.len() != self.targets.len() {
            return Err(AruError::msg(
                "instruction source targets contains duplicates",
            ));
        }
        for target in &self.targets {
            if !project_targets.contains(target) {
                return Err(AruError::msg(format!(
                    "instruction source target {target:?} is not declared in project.targets"
                )));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Instructions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<InstructionSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillRequirement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default = "wildcard")]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub paths: BTreeMap<String, String>,
}

fn wildcard() -> Vec<String> {
    vec!["*".to_owned()]
}

impl Default for SkillRequirement {
    fn default() -> Self {
        Self {
            version: None,
            branch: None,
            rev: None,
            include: wildcard(),
            exclude: Vec::new(),
            paths: BTreeMap::new(),
        }
    }
}

impl SkillRequirement {
    pub fn is_wildcard(&self) -> bool {
        self.include.iter().any(|name| name == "*")
    }

    pub fn normalize(&mut self) {
        sort_dedup(&mut self.include);
        sort_dedup(&mut self.exclude);
        if self.is_wildcard() {
            self.include = wildcard();
        } else {
            self.exclude.clear();
        }
    }

    pub fn validate(&self, source: &str) -> Result<()> {
        let references = usize::from(self.version.is_some())
            + usize::from(self.branch.is_some())
            + usize::from(self.rev.is_some());
        if references > 1 {
            return Err(AruError::msg(format!(
                "skill source {source:?} can set only one of version, branch, or rev"
            )));
        }
        if let Some(branch) = &self.branch {
            validate_branch_name(branch)?;
        }
        if self.include.is_empty() {
            return Err(AruError::msg(format!(
                "skill source {source:?} must include at least one skill or *"
            )));
        }
        if self.include.len() > 1 && self.is_wildcard() {
            return Err(AruError::msg(format!(
                "skill source {source:?} cannot combine * with explicit names"
            )));
        }
        for name in self
            .include
            .iter()
            .chain(self.exclude.iter())
            .filter(|name| name.as_str() != "*")
        {
            validate_name(name, "skill name")?;
        }
        for (name, path) in &self.paths {
            validate_name(name, "skill name")?;
            crate::skill::validate_relative_selector(path)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpRequirement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(
        default,
        rename = "package-registry",
        skip_serializing_if = "Option::is_none"
    )]
    pub package_registry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(
        default,
        rename = "bearer-token-env",
        skip_serializing_if = "Option::is_none"
    )]
    pub bearer_token_env: Option<String>,
}

impl McpRequirement {
    pub fn validate(&self, name: &str) -> Result<()> {
        validate_name(name, "MCP name")?;
        let source_count = usize::from(self.server.is_some())
            + usize::from(self.url.is_some())
            + usize::from(self.command.is_some());
        if source_count != 1 {
            return Err(AruError::msg(format!(
                "MCP {name:?} must set exactly one of server, url, or command"
            )));
        }
        if self.command.is_none() && !self.args.is_empty() {
            return Err(AruError::msg(format!(
                "MCP {name:?} args require a direct stdio command"
            )));
        }
        if let Some(command) = &self.command {
            if command.is_empty() || command.contains('\0') {
                return Err(AruError::msg(format!(
                    "direct stdio MCP {name:?} command must be non-empty and contain no NUL"
                )));
            }
            if self.args.iter().any(|argument| argument.contains('\0')) {
                return Err(AruError::msg(format!(
                    "direct stdio MCP {name:?} arguments must contain no NUL"
                )));
            }
            if self.version.is_some() {
                return Err(AruError::msg(format!(
                    "direct stdio MCP {name:?} cannot set version"
                )));
            }
            if self.registry.is_some() || self.package_registry.is_some() {
                return Err(AruError::msg(format!(
                    "direct stdio MCP {name:?} cannot set registry or package-registry"
                )));
            }
            if self.bearer_token_env.is_some() {
                return Err(AruError::msg(format!(
                    "direct stdio MCP {name:?} cannot set bearer-token-env"
                )));
            }
            if self
                .transport
                .as_deref()
                .is_some_and(|transport| transport != "stdio")
            {
                return Err(AruError::msg(format!(
                    "direct stdio MCP {name:?} requires stdio transport"
                )));
            }
        } else if self.url.is_some() {
            if self.version.is_some() || self.package_registry.is_some() {
                return Err(AruError::msg(format!(
                    "direct MCP {name:?} cannot set version or package-registry"
                )));
            }
            validate_https_url(self.url.as_deref().unwrap(), "MCP URL")?;
        } else {
            validate_https_url(
                self.registry
                    .as_deref()
                    .unwrap_or(crate::registry::DEFAULT_REGISTRY),
                "registry URL",
            )?;
        }
        if let Some(env) = &self.bearer_token_env {
            validate_env_name(env)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub project: Project,
    #[serde(default)]
    pub instructions: Instructions,
    #[serde(default)]
    pub skills: BTreeMap<String, SkillRequirement>,
    #[serde(default)]
    pub mcp: BTreeMap<String, McpRequirement>,
}

impl Manifest {
    pub fn validate(&self) -> Result<()> {
        if self.project.targets.is_empty() {
            return Err(AruError::msg("project.targets must not be empty"));
        }
        let unique: BTreeSet<_> = self.project.targets.iter().collect();
        if unique.len() != self.project.targets.len() {
            return Err(AruError::msg("project.targets contains duplicates"));
        }
        for source in &self.instructions.sources {
            source.validate(&self.project.targets)?;
        }
        for (source, requirement) in &self.skills {
            requirement.validate(source)?;
        }
        for (name, requirement) in &self.mcp {
            requirement.validate(name)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ManifestDocument {
    path: PathBuf,
    doc: DocumentMut,
}

impl ManifestDocument {
    pub fn load(project: &Path) -> Result<Self> {
        let path = project.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path).at(&path)?;
        let doc = text.parse::<DocumentMut>().map_err(|error| {
            AruError::msg(format!(
                "invalid editable TOML in {}: {error}",
                path.display()
            ))
        })?;
        for key in ["project", "instructions", "skills", "mcp"] {
            if doc.get(key).is_some_and(|item| !item.is_table()) {
                return Err(AruError::msg(format!(
                    "{key} must use a TOML table so aru can edit it without losing unrelated content"
                )));
            }
        }
        let this = Self { path, doc };
        this.manifest()?;
        Ok(this)
    }

    pub fn new(targets: &[Target]) -> Self {
        let mut doc = DocumentMut::new();
        let mut project = Table::new();
        project["targets"] = Item::Value(target_array(targets).into());
        doc["project"] = Item::Table(project);
        doc["instructions"] = Item::Table(Table::new());
        doc["skills"] = Item::Table(Table::new());
        doc["mcp"] = Item::Table(Table::new());
        Self {
            path: PathBuf::from(MANIFEST_FILE),
            doc,
        }
    }

    pub fn manifest(&self) -> Result<Manifest> {
        let manifest: Manifest =
            toml::from_str(&self.doc.to_string()).map_err(|source| AruError::Toml {
                path: self.path.clone(),
                source,
            })?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn set_targets(&mut self, targets: &[Target]) {
        let decor = self.doc["project"]["targets"]
            .as_value()
            .map(|value| value.decor().clone());
        let mut value = Value::Array(target_array(targets));
        if let Some(decor) = decor {
            *value.decor_mut() = decor;
        }
        self.doc["project"]["targets"] = Item::Value(value);
    }

    pub fn set_instruction_sources(&mut self, sources: &[InstructionSource]) {
        ensure_table(&mut self.doc, "instructions");
        let mut array = ArrayOfTables::new();
        for source in sources {
            let mut table = Table::new();
            table["files"] = Item::Value(string_array(&source.files).into());
            if !source.exclude.is_empty() {
                table["exclude"] = Item::Value(string_array(&source.exclude).into());
            }
            if let Some(scope) = source.scope {
                table["scope"] = toml_edit::value(match scope {
                    InstructionSourceScope::SourceDirectory => "source-directory",
                });
            }
            if !source.apply_to.is_empty() {
                table["apply-to"] = Item::Value(string_array(&source.apply_to).into());
            }
            if !source.targets.is_empty() {
                table["targets"] = Item::Value(target_array(&source.targets).into());
            }
            array.push(table);
        }
        self.doc["instructions"]["sources"] = Item::ArrayOfTables(array);
    }

    pub fn set_skill(&mut self, source: &str, requirement: &SkillRequirement) {
        ensure_table(&mut self.doc, "skills");
        self.doc["skills"][source] = Item::Value(skill_inline(requirement).into());
    }

    pub fn remove_skill(&mut self, source: &str) {
        if let Some(table) = self.doc["skills"].as_table_mut() {
            table.remove(source);
        }
    }

    pub fn set_mcp(&mut self, name: &str, requirement: &McpRequirement) {
        ensure_table(&mut self.doc, "mcp");
        self.doc["mcp"][name] = Item::Table(mcp_table(requirement));
    }

    pub fn remove_mcp(&mut self, name: &str) {
        if let Some(table) = self.doc["mcp"].as_table_mut() {
            table.remove(name);
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.doc.to_string().into_bytes()
    }
}

fn ensure_table(doc: &mut DocumentMut, key: &str) {
    if !doc[key].is_table() {
        doc[key] = Item::Table(Table::new());
    }
}

fn target_array(targets: &[Target]) -> Array {
    let mut array = Array::new();
    for target in targets {
        array.push(target.to_string());
    }
    array
}

fn skill_inline(requirement: &SkillRequirement) -> InlineTable {
    let mut table = InlineTable::new();
    if let Some(version) = &requirement.version {
        table.insert("version", Value::from(version.as_str()));
    }
    if let Some(branch) = &requirement.branch {
        table.insert("branch", Value::from(branch.as_str()));
    }
    if let Some(rev) = &requirement.rev {
        table.insert("rev", Value::from(rev.as_str()));
    }
    table.insert("include", Value::Array(string_array(&requirement.include)));
    table.insert("exclude", Value::Array(string_array(&requirement.exclude)));
    if !requirement.paths.is_empty() {
        let mut paths = InlineTable::new();
        for (name, path) in &requirement.paths {
            paths.insert(name, Value::from(path.as_str()));
        }
        table.insert("paths", Value::InlineTable(paths));
    }
    table
}

fn mcp_table(requirement: &McpRequirement) -> Table {
    let mut table = Table::new();
    for (key, value) in [
        ("registry", requirement.registry.as_ref()),
        ("server", requirement.server.as_ref()),
        ("version", requirement.version.as_ref()),
        ("transport", requirement.transport.as_ref()),
        ("package-registry", requirement.package_registry.as_ref()),
        ("url", requirement.url.as_ref()),
        ("command", requirement.command.as_ref()),
        ("bearer-token-env", requirement.bearer_token_env.as_ref()),
    ] {
        if let Some(value) = value {
            table[key] = toml_edit::value(value.as_str());
        }
    }
    if !requirement.args.is_empty() {
        table["args"] = Item::Value(string_array(&requirement.args).into());
    }
    table
}

fn string_array(values: &[String]) -> Array {
    let mut array = Array::new();
    for value in values {
        array.push(value.as_str());
    }
    array
}

fn validate_portable_pattern(pattern: &str) -> Result<()> {
    let valid = !pattern.is_empty()
        && !pattern.starts_with('/')
        && !pattern.contains('\\')
        && !pattern.chars().any(char::is_control)
        && pattern
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..");
    if valid {
        globset::Glob::new(pattern).map_err(|error| {
            AruError::msg(format!("invalid instruction glob {pattern:?}: {error}"))
        })?;
        Ok(())
    } else {
        Err(AruError::msg(format!(
            "invalid instruction path/glob {pattern:?}; use a portable project-relative pattern"
        )))
    }
}

pub fn validate_branch_name(name: &str) -> Result<()> {
    let invalid_character = name.chars().any(|character| {
        character.is_control() || character.is_whitespace() || "~^:?*[\\".contains(character)
    });
    let invalid_component = name.split('/').any(|component| {
        component.is_empty() || component.starts_with('.') || component.ends_with(".lock")
    });
    let valid = !name.is_empty()
        && name.len() <= 255
        && name != "@"
        && !name.starts_with('-')
        && !name.starts_with('/')
        && !name.ends_with('/')
        && !name.ends_with('.')
        && !name.contains("..")
        && !name.contains("@{")
        && !invalid_character
        && !invalid_component;
    if valid {
        Ok(())
    } else {
        Err(AruError::msg(format!("invalid Git branch name {name:?}")))
    }
}

pub fn validate_name(name: &str, kind: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && !name.starts_with('-')
        && !name.ends_with('-')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(AruError::msg(format!(
            "invalid {kind} {name:?}; use 1-64 lowercase letters, digits, or interior hyphens"
        )))
    }
}

pub fn validate_env_name(name: &str) -> Result<()> {
    let mut bytes = name.bytes();
    let valid = bytes
        .next()
        .is_some_and(|first| first.is_ascii_uppercase() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(AruError::msg(format!(
            "invalid secret environment variable name {name:?}"
        )))
    }
}

pub fn validate_https_url(value: &str, kind: &str) -> Result<()> {
    let parsed = url::Url::parse(value)
        .map_err(|_| AruError::msg(format!("invalid {kind}; expected an HTTPS URL")))?;
    if parsed.scheme() != "https" || !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AruError::msg(format!(
            "invalid {kind}; HTTPS without URL userinfo is required"
        )));
    }
    Ok(())
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_preserves_unrelated_comments() {
        let text = "# heading\nfuture = 1\n\n[project]\ntargets = [\"codex\"] # keep\n\n[skills]\n# package note\n\"owner/repo\" = { include = [\"old\"] }\n\n[custom]\nanswer = 42\n";
        let doc = text.parse::<DocumentMut>().unwrap();
        let mut document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc,
        };
        document.set_skill(
            "owner/repo",
            &SkillRequirement {
                include: vec!["new".into()],
                ..SkillRequirement::default()
            },
        );
        let output = String::from_utf8(document.bytes()).unwrap();
        assert!(output.contains("# heading"));
        assert!(output.contains("# package note"));
        assert!(output.contains("[custom]\nanswer = 42"));
        assert!(output.contains("include = [\"new\"]"));
    }

    #[test]
    fn target_mutation_preserves_the_key_comment_and_unrelated_content() {
        let text = "# heading\nfuture = 1\n\n[project]\ntargets = [\"codex\"] # why this set exists\n\n[custom]\nanswer = 42\n";
        let mut document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: text.parse().unwrap(),
        };

        document.set_targets(&[Target::Codex, Target::Claude]);

        let output = String::from_utf8(document.bytes()).unwrap();
        assert!(output.contains("targets = [\"codex\", \"claude\"] # why this set exists"));
        assert!(output.starts_with("# heading\nfuture = 1"));
        assert!(output.contains("[custom]\nanswer = 42"));
    }

    #[test]
    fn branch_fixture_round_trips_without_manifest_schema() {
        let fixture = include_str!("../tests/fixtures/contracts/aru-branch.toml");
        let document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: fixture.parse().unwrap(),
        };
        let manifest = document.manifest().unwrap();
        assert_eq!(
            manifest.skills["owner/repository"].branch.as_deref(),
            Some("main")
        );
        assert_eq!(document.bytes(), fixture.as_bytes());
        assert!(!fixture.contains("schema ="));
        assert!(
            !String::from_utf8(ManifestDocument::new(&[Target::Codex]).bytes())
                .unwrap()
                .contains("schema =")
        );
    }

    #[test]
    fn branch_mutation_preserves_comments_and_reference_kinds_are_exclusive() {
        let text = "# keep\nfuture = 999\n\n[project]\ntargets = [\"codex\"]\n\n[skills]\n";
        let mut document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: text.parse().unwrap(),
        };
        document.set_skill(
            "owner/repo",
            &SkillRequirement {
                branch: Some("main".into()),
                ..SkillRequirement::default()
            },
        );
        assert!(document.manifest().is_ok());
        let output = String::from_utf8(document.bytes()).unwrap();
        assert!(output.starts_with("# keep\nfuture = 999"));
        assert!(output.contains("branch = \"main\""));

        let invalid = SkillRequirement {
            version: Some("1.0.0".into()),
            branch: Some("main".into()),
            ..SkillRequirement::default()
        };
        assert!(invalid.validate("owner/repo").is_err());
    }

    #[test]
    fn instruction_sources_parse_and_validate_scope_and_targets() {
        let text = r#"
[project]
targets = ["codex", "claude", "copilot", "pi", "opencode"]

[[instructions.sources]]
files = ["AGENTS.md", "src/**/AGENTS.md"]
exclude = ["target/**"]
scope = "source-directory"

[[instructions.sources]]
files = ["docs/rust.md"]
apply-to = ["**/*.rs"]
targets = ["claude", "copilot"]
"#;
        let document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: text.parse().unwrap(),
        };
        let manifest = document.manifest().unwrap();
        assert_eq!(manifest.instructions.sources.len(), 2);
        assert_eq!(
            manifest.instructions.sources[1].targets,
            [Target::Claude, Target::Copilot]
        );
        assert_eq!(document.bytes(), text.as_bytes());
    }

    #[test]
    fn instruction_source_rejects_ambiguous_scope_and_undeclared_target() {
        let source = InstructionSource {
            files: vec!["AGENTS.md".into()],
            exclude: Vec::new(),
            scope: Some(InstructionSourceScope::SourceDirectory),
            apply_to: vec!["**/*.rs".into()],
            targets: Vec::new(),
        };
        assert!(
            source
                .validate(&[Target::Codex, Target::Claude])
                .unwrap_err()
                .to_string()
                .contains("exactly one")
        );
        let source = InstructionSource {
            scope: None,
            apply_to: vec!["**/*.rs".into()],
            targets: vec![Target::Copilot],
            ..source
        };
        assert!(
            source
                .validate(&[Target::Claude])
                .unwrap_err()
                .to_string()
                .contains("not declared")
        );
    }

    #[test]
    fn instruction_mutation_preserves_unrelated_manifest_content() {
        let text = "# keep\n[project]\ntargets = [\"claude\"]\n\n[instructions]\n# replace only sources\n\n[custom]\nanswer = 42\n";
        let mut document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: text.parse().unwrap(),
        };
        document.set_instruction_sources(&[InstructionSource {
            files: vec!["AGENTS.md".into()],
            exclude: Vec::new(),
            scope: Some(InstructionSourceScope::SourceDirectory),
            apply_to: Vec::new(),
            targets: Vec::new(),
        }]);
        let output = String::from_utf8(document.bytes()).unwrap();
        assert!(output.starts_with("# keep"));
        assert!(output.contains("# replace only sources"));
        assert!(output.contains("[custom]\nanswer = 42"));
        assert!(output.contains("files = [\"AGENTS.md\"]"));
        assert!(document.manifest().is_ok());
    }

    #[test]
    fn manifest_fixture_parses_and_preserves_comments() {
        let fixture = include_str!("../tests/fixtures/contracts/aru.toml");
        let document = ManifestDocument {
            path: PathBuf::from("aru.toml"),
            doc: fixture.parse().unwrap(),
        };
        let manifest = document.manifest().unwrap();
        assert_eq!(manifest.project.targets.len(), 2);
        assert_eq!(
            manifest.skills["owner/repository"].paths["writing-plans"],
            "skills/writing-plans"
        );
        assert_eq!(document.bytes(), fixture.as_bytes());
    }
}
