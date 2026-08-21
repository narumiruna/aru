use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::error::{AruError, IoContext, Result};

pub const MANIFEST_FILE: &str = "aru.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    Agents,
    Codex,
    Claude,
    Copilot,
    Opencode,
    Pi,
    Amp,
    Antigravity,
    Cline,
    Cursor,
    Deepagents,
    Dexto,
    Firebender,
    Gemini,
    Kimi,
    Loaf,
    Promptscript,
    Replit,
    Warp,
    Zed,
    Adal,
    AiderDesk,
    Astrbot,
    Autohand,
    Augment,
    Bob,
    Openclaw,
    Codearts,
    Codebuddy,
    Codemaker,
    Codestudio,
    Commandcode,
    Continue,
    Cortex,
    Crush,
    Devin,
    Droid,
    Eve,
    Forge,
    Goose,
    Grok,
    Hermes,
    InferenceSh,
    Jazz,
    Junie,
    Iflow,
    Kilo,
    Kimchi,
    Kiro,
    Kode,
    Lingma,
    Mcpjam,
    Minimax,
    Vibe,
    Moxby,
    Mux,
    Openhands,
    Ona,
    Posit,
    Qoder,
    Qwen,
    Reasonix,
    Rovodev,
    Roo,
    Tabnine,
    Terramind,
    Tinycloud,
    Trae,
    Windsurf,
    Zcode,
    Zencoder,
    Neovate,
    Pochi,
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(crate::target::spec(*self).name)
    }
}

impl std::str::FromStr for Target {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        crate::target::parse(value).ok_or_else(|| {
            format!(
                "unknown target {value:?}; run `aru target list --available` to list supported targets"
            )
        })
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
            if crate::target::capabilities(*target).instructions.is_none() {
                return Err(AruError::msg(format!(
                    "instruction source target {target} does not support instructions"
                )));
            }
        }
        if self.targets.is_empty() && crate::target::instruction_targets(project_targets).is_empty()
        {
            return Err(AruError::msg(
                "instruction source has no configured target that supports instructions",
            ));
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<Target>>,
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
            targets: None,
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
        if let Some(targets) = &mut self.targets {
            targets.sort();
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

    pub fn validate_targets(&self, source: &str, project_targets: &[Target]) -> Result<()> {
        validate_dependency_targets(
            self.targets.as_deref(),
            project_targets,
            &format!("skill source {source:?}"),
            |target| crate::target::capabilities(target).skills,
            "Agent Skills",
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageRequirement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<Target>>,
}

impl PackageRequirement {
    pub fn normalize(&mut self) {
        if let Some(targets) = &mut self.targets {
            targets.sort();
        }
    }

    pub fn validate(&self, source: &str, parent_targets: &[Target]) -> Result<()> {
        let references = usize::from(self.version.is_some())
            + usize::from(self.branch.is_some())
            + usize::from(self.rev.is_some());
        if references > 1 {
            return Err(AruError::msg(format!(
                "aru package {source:?} can set only one of version, branch, or rev"
            )));
        }
        if let Some(version) = &self.version {
            semver::VersionReq::parse(version).map_err(|error| {
                AruError::msg(format!(
                    "invalid aru package SemVer requirement {version:?}: {error}"
                ))
            })?;
        }
        if let Some(branch) = &self.branch {
            validate_branch_name(branch)?;
        }
        if let Some(revision) = &self.rev {
            let valid = (7..=40).contains(&revision.len())
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit());
            if !valid {
                return Err(AruError::msg(format!(
                    "invalid aru package Git revision {revision:?}"
                )));
            }
        }
        validate_dependency_targets(
            self.targets.as_deref(),
            parent_targets,
            &format!("aru package {source:?}"),
            |_| true,
            "configured targets",
        )
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageTrust {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mcp: Vec<String>,
}

impl PackageTrust {
    pub fn normalize(&mut self) {
        sort_dedup(&mut self.mcp);
    }

    pub fn validate(&self, source: &str) -> Result<()> {
        if self.mcp.is_empty() {
            return Err(AruError::msg(format!(
                "package trust {source:?} must name at least one MCP server"
            )));
        }
        if self.mcp.iter().collect::<BTreeSet<_>>().len() != self.mcp.len() {
            return Err(AruError::msg(format!(
                "package trust {source:?} contains duplicate MCP names"
            )));
        }
        for name in &self.mcp {
            validate_name(name, "trusted package MCP name")?;
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
    #[serde(default, rename = "env-vars", skip_serializing_if = "Vec::is_empty")]
    pub env_vars: Vec<String>,
    #[serde(
        default,
        rename = "env-http-headers",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub env_http_headers: BTreeMap<String, String>,
    #[serde(
        default,
        rename = "bearer-token-env",
        skip_serializing_if = "Option::is_none"
    )]
    pub bearer_token_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targets: Option<Vec<Target>>,
}

impl McpRequirement {
    pub fn normalize(&mut self) {
        self.env_vars.sort();
        if let Some(targets) = &mut self.targets {
            targets.sort();
        }
    }

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
        if self.command.is_none() && !self.env_vars.is_empty() {
            return Err(AruError::msg(format!(
                "MCP {name:?} env-vars require a direct stdio command"
            )));
        }
        if self.url.is_none() && !self.env_http_headers.is_empty() {
            return Err(AruError::msg(format!(
                "MCP {name:?} env-http-headers require a direct URL"
            )));
        }
        let unique_env: BTreeSet<_> = self.env_vars.iter().collect();
        if unique_env.len() != self.env_vars.len() {
            return Err(AruError::msg(format!(
                "MCP {name:?} env-vars contains duplicates"
            )));
        }
        for env in &self.env_vars {
            validate_env_name(env)?;
        }
        let mut unique_headers = BTreeSet::new();
        for (header, env) in &self.env_http_headers {
            validate_http_header_name(header)?;
            if !unique_headers.insert(header.to_ascii_lowercase()) {
                return Err(AruError::msg(format!(
                    "MCP {name:?} env-http-headers contains duplicate header {header:?}"
                )));
            }
            validate_env_name(env)?;
        }
        if self.bearer_token_env.is_some()
            && self
                .env_http_headers
                .keys()
                .any(|header| header.eq_ignore_ascii_case("authorization"))
        {
            return Err(AruError::msg(format!(
                "MCP {name:?} cannot combine bearer-token-env with an Authorization env-http-header"
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

    pub fn validate_targets(&self, name: &str, project_targets: &[Target]) -> Result<()> {
        validate_dependency_targets(
            self.targets.as_deref(),
            project_targets,
            &format!("MCP {name:?}"),
            |target| crate::target::capabilities(target).mcp,
            "MCP",
        )
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
    #[serde(default)]
    pub packages: BTreeMap<String, PackageRequirement>,
    #[serde(default, rename = "package-trust")]
    pub package_trust: BTreeMap<String, PackageTrust>,
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
            requirement.validate_targets(source, &self.project.targets)?;
        }
        for (name, requirement) in &self.mcp {
            requirement.validate(name)?;
            requirement.validate_targets(name, &self.project.targets)?;
        }
        for (source, requirement) in &self.packages {
            requirement.validate(source, &self.project.targets)?;
        }
        for (source, trust) in &self.package_trust {
            trust.validate(source)?;
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
        for key in [
            "project",
            "instructions",
            "skills",
            "mcp",
            "packages",
            "package-trust",
        ] {
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
        table_mut_or_insert(&mut self.doc, "instructions")["sources"] = Item::ArrayOfTables(array);
    }

    pub fn set_skill(&mut self, source: &str, requirement: &SkillRequirement) {
        table_mut_or_insert(&mut self.doc, "skills")[source] =
            Item::Value(skill_inline(requirement).into());
    }

    pub fn remove_skill(&mut self, source: &str) {
        if let Some(table) = existing_table_mut(&mut self.doc, "skills") {
            table.remove(source);
        }
    }

    pub fn set_mcp(&mut self, name: &str, requirement: &McpRequirement) {
        table_mut_or_insert(&mut self.doc, "mcp")[name] = Item::Table(mcp_table(requirement));
    }

    pub fn remove_mcp(&mut self, name: &str) {
        if let Some(table) = existing_table_mut(&mut self.doc, "mcp") {
            table.remove(name);
        }
    }

    pub fn set_package(&mut self, source: &str, requirement: &PackageRequirement) {
        table_mut_or_insert(&mut self.doc, "packages")[source] =
            Item::Value(package_inline(requirement).into());
    }

    pub fn remove_package(&mut self, source: &str) {
        if let Some(table) = existing_table_mut(&mut self.doc, "packages") {
            table.remove(source);
        }
    }

    pub fn set_package_trust(&mut self, source: &str, trust: &PackageTrust) {
        let mut table = Table::new();
        if !trust.mcp.is_empty() {
            table["mcp"] = Item::Value(string_array(&trust.mcp).into());
        }
        table_mut_or_insert(&mut self.doc, "package-trust")[source] = Item::Table(table);
    }

    pub fn remove_package_trust(&mut self, source: &str) {
        if let Some(table) = existing_table_mut(&mut self.doc, "package-trust") {
            table.remove(source);
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.doc.to_string().into_bytes()
    }
}

fn table_mut_or_insert<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut Table {
    if doc.get(key).is_none() {
        doc[key] = Item::Table(Table::new());
    }
    existing_table_mut(doc, key).expect("ManifestDocument optional sections are TOML tables")
}

fn existing_table_mut<'a>(doc: &'a mut DocumentMut, key: &str) -> Option<&'a mut Table> {
    doc.get_mut(key).and_then(Item::as_table_mut)
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
    if let Some(targets) = &requirement.targets {
        table.insert("targets", Value::Array(target_array(targets)));
    }
    table
}

fn package_inline(requirement: &PackageRequirement) -> InlineTable {
    let mut table = InlineTable::new();
    if let Some(version) = &requirement.version {
        table.insert("version", Value::from(version.as_str()));
    }
    if let Some(branch) = &requirement.branch {
        table.insert("branch", Value::from(branch.as_str()));
    }
    if let Some(revision) = &requirement.rev {
        table.insert("rev", Value::from(revision.as_str()));
    }
    if let Some(targets) = &requirement.targets {
        table.insert("targets", Value::Array(target_array(targets)));
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
    if !requirement.env_vars.is_empty() {
        table["env-vars"] = Item::Value(string_array(&requirement.env_vars).into());
    }
    if !requirement.env_http_headers.is_empty() {
        let mut headers = Table::new();
        for (header, env) in &requirement.env_http_headers {
            headers[header] = toml_edit::value(env.as_str());
        }
        table["env-http-headers"] = Item::Table(headers);
    }
    if let Some(targets) = &requirement.targets {
        table["targets"] = Item::Value(target_array(targets).into());
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
            "invalid environment variable name {name:?}"
        )))
    }
}

pub fn validate_http_header_name(name: &str) -> Result<()> {
    reqwest::header::HeaderName::from_bytes(name.as_bytes())
        .map(|_| ())
        .map_err(|_| AruError::msg(format!("invalid HTTP header name {name:?}")))
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

fn validate_dependency_targets(
    targets: Option<&[Target]>,
    project_targets: &[Target],
    resource: &str,
    supports: impl Fn(Target) -> bool,
    capability: &str,
) -> Result<()> {
    let Some(targets) = targets else {
        return Ok(());
    };
    if targets.is_empty() {
        return Err(AruError::msg(format!(
            "{resource} dependency targets must not be empty"
        )));
    }
    let unique: BTreeSet<_> = targets.iter().collect();
    if unique.len() != targets.len() {
        return Err(AruError::msg(format!(
            "{resource} dependency targets contains duplicates"
        )));
    }
    for target in targets {
        if !project_targets.contains(target) {
            return Err(AruError::msg(format!(
                "{resource} dependency target {target} is not declared in project.targets"
            )));
        }
        if !supports(*target) {
            return Err(AruError::msg(format!(
                "{resource} dependency target {target} does not support {capability}"
            )));
        }
    }
    Ok(())
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort();
    values.dedup();
}

#[cfg(test)]
mod tests;
