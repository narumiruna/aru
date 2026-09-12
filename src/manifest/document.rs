use std::path::{Path, PathBuf};

use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::error::{AruError, IoContext, Result};

use super::{
    InstructionSource, InstructionSourceScope, MANIFEST_FILE, Manifest, McpRequirement,
    PackageRequirement, PackageTrust, PluginRequirement, PluginTrust, SkillRequirement, Target,
};

#[derive(Debug, Clone)]
pub struct ManifestDocument {
    pub(super) path: PathBuf,
    pub(super) doc: DocumentMut,
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
            "plugins",
            "plugin-trust",
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

    pub fn set_plugin(&mut self, name: &str, requirement: &PluginRequirement) {
        let mut table = Table::new();
        table["source"] = toml_edit::value(requirement.source.as_str());
        table["format"] = toml_edit::value(requirement.format.to_string());
        for (key, value) in [
            ("subdir", requirement.subdir.as_ref()),
            ("version", requirement.version.as_ref()),
            ("branch", requirement.branch.as_ref()),
            ("rev", requirement.rev.as_ref()),
        ] {
            if let Some(value) = value {
                table[key] = toml_edit::value(value.as_str());
            }
        }
        if !requirement.components.is_empty() {
            table["components"] = Item::Value(
                string_array(
                    &requirement
                        .components
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                )
                .into(),
            );
        }
        if !requirement.skills.is_empty() {
            table["skills"] = Item::Value(string_array(&requirement.skills).into());
        }
        if !requirement.mcp.is_empty() {
            table["mcp"] = Item::Value(string_array(&requirement.mcp).into());
        }
        if let Some(targets) = &requirement.targets {
            table["targets"] = Item::Value(target_array(targets).into());
        }
        table_mut_or_insert(&mut self.doc, "plugins")[name] = Item::Table(table);
    }

    pub fn remove_plugin(&mut self, name: &str) {
        if let Some(table) = existing_table_mut(&mut self.doc, "plugins") {
            table.remove(name);
        }
    }

    pub fn set_plugin_trust(&mut self, name: &str, trust: &PluginTrust) {
        let mut table = Table::new();
        if !trust.mcp.is_empty() {
            table["mcp"] = Item::Value(string_array(&trust.mcp).into());
        }
        table_mut_or_insert(&mut self.doc, "plugin-trust")[name] = Item::Table(table);
    }

    pub fn remove_plugin_trust(&mut self, name: &str) {
        if let Some(table) = existing_table_mut(&mut self.doc, "plugin-trust") {
            table.remove(name);
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
