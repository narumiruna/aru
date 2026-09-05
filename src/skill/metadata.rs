//! Local frontmatter overrides. Source hashes remain byte-for-byte tree hashes.
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

use super::{SKILL_MD_MAX_BYTES, skill_digest_with_document};
use crate::error::{AruError, IoContext, Result};

mod value;
use value::parse as parse_yaml;

type Fields = BTreeMap<String, Value>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct MetadataState {
    /// Complete locked source tree digest, distinct from the local projection.
    pub source_digest: String,
    /// Exact last-applied header, including both delimiters and their newlines.
    pub frontmatter: String,
    /// YAML values keyed by top-level field; a whole nested value is overridden.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub removed: BTreeSet<String>,
}

impl MetadataState {
    pub(crate) fn new(document: &Document) -> Self {
        Self {
            source_digest: String::new(),
            frontmatter: document.frontmatter.clone(),
            values: BTreeMap::new(),
            removed: BTreeSet::new(),
        }
    }

    pub(crate) fn has_overrides(&self) -> bool {
        !self.values.is_empty() || !self.removed.is_empty()
    }

    fn validate_size(&self) -> Result<()> {
        let bytes = self
            .values
            .iter()
            .map(|(key, value)| key.len() + value.len())
            .chain(self.removed.iter().map(String::len))
            .sum::<usize>();
        if self.values.len() + self.removed.len() > 20_000 || bytes as u64 > SKILL_MD_MAX_BYTES {
            return Err(AruError::msg(
                "local skill metadata overrides exceed storage limits",
            ));
        }
        Ok(())
    }

    /// Reconstitute the last applied raw bytes to prove that only metadata changed.
    pub(crate) fn matches(
        &self,
        root: &Path,
        current: &Document,
        applied_digest: &str,
    ) -> Result<bool> {
        let applied = Document::parse(&self.frontmatter)?;
        if ["name", "description"]
            .iter()
            .any(|key| applied.fields[*key] != current.fields[*key])
        {
            return Ok(false);
        }
        let restored = format!("{}{}", self.frontmatter, current.body);
        Ok(skill_digest_with_document(root, restored.as_bytes())? == applied_digest)
    }

    pub(crate) fn merge(
        &self,
        current: Option<&Document>,
        source: &Document,
    ) -> Result<(Document, Self)> {
        self.validate_size()?;
        let applied = Document::parse(&self.frontmatter)?;
        let mut next = self.clone();
        if let Some(current) = current {
            let keys: BTreeSet<_> = applied.fields.keys().chain(current.fields.keys()).collect();
            for key in keys.into_iter().filter(|key| !protected(key)) {
                if applied.fields.get(key) == current.fields.get(key) {
                    continue;
                }
                match current.fields.get(key) {
                    Some(value) => {
                        next.values.insert(key.clone(), yaml(value)?);
                        next.removed.remove(key);
                    }
                    None => {
                        next.values.remove(key);
                        next.removed.insert(key.clone());
                    }
                }
            }
        }
        next.validate_size()?;
        let mut fields = source.fields.clone();
        for (key, value) in &next.values {
            if protected(key) || next.removed.contains(key) {
                return Err(AruError::msg("invalid local skill metadata override state"));
            }
            fields.insert(key.clone(), parse_yaml(value)?);
        }
        for key in &next.removed {
            if protected(key) {
                return Err(AruError::msg("invalid local skill metadata removal state"));
            }
            fields.remove(key);
        }
        // Avoid rewriting a user's header when only upstream body/assets changed.
        let mut frontmatter =
            if let Some(current) = current.filter(|current| current.fields == fields) {
                current.frontmatter.clone()
            } else if source.fields == fields {
                source.frontmatter.clone()
            } else {
                format!("---\n{}---\n", yaml(&fields)?)
            };
        if !source.body.is_empty() && !frontmatter.ends_with('\n') {
            frontmatter.push('\n');
        }
        let document = Document::parse(&format!("{frontmatter}{}", source.body))?;
        next.frontmatter = document.frontmatter.clone();
        Ok((document, next))
    }
}

pub(crate) struct Document {
    pub(crate) frontmatter: String,
    pub(crate) body: String,
    pub(crate) fields: Fields,
}

impl Document {
    pub(crate) fn read(path: &Path) -> Result<Self> {
        let metadata = std::fs::symlink_metadata(path).at(path)?;
        if !metadata.file_type().is_file() || metadata.len() > SKILL_MD_MAX_BYTES {
            return Err(AruError::msg(format!(
                "SKILL.md must be a regular file no larger than {SKILL_MD_MAX_BYTES} bytes: {}",
                path.display()
            )));
        }
        let mut text = String::new();
        std::fs::File::open(path)
            .at(path)?
            .take(SKILL_MD_MAX_BYTES + 1)
            .read_to_string(&mut text)
            .at(path)?;
        Self::parse(&text)
    }

    pub(crate) fn parse(text: &str) -> Result<Self> {
        if text.len() as u64 > SKILL_MD_MAX_BYTES {
            return Err(AruError::msg("SKILL.md exceeds byte limit"));
        }
        let yaml_body = text
            .strip_prefix("---\n")
            .or_else(|| text.strip_prefix("---\r\n"))
            .ok_or_else(|| AruError::msg("SKILL.md has no YAML frontmatter"))?;
        let start = text.len() - yaml_body.len();
        let mut offset = 0;
        for line in yaml_body.split_inclusive('\n') {
            if line.trim_end_matches(['\r', '\n']) == "---" {
                let value = parse_yaml(&yaml_body[..offset])?;
                let mapping = value
                    .as_mapping()
                    .ok_or_else(|| AruError::msg("SKILL.md frontmatter must be a YAML mapping"))?;
                let mut fields = Fields::new();
                for (key, value) in mapping {
                    let key = key.as_str().ok_or_else(|| {
                        AruError::msg("SKILL.md frontmatter keys must be strings")
                    })?;
                    fields.insert(key.to_owned(), value.clone());
                }
                let name = fields
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AruError::msg("SKILL.md name must be a string"))?;
                crate::manifest::validate_name(name, "skill name")?;
                let description = fields
                    .get("description")
                    .and_then(Value::as_str)
                    .ok_or_else(|| AruError::msg("SKILL.md description must be a string"))?;
                if description.trim().is_empty() || description.len() > 1024 {
                    return Err(AruError::msg(
                        "SKILL.md description must contain 1-1024 UTF-8 bytes",
                    ));
                }
                let end = start + offset + line.len();
                return Ok(Self {
                    frontmatter: text[..end].into(),
                    body: text[end..].into(),
                    fields,
                });
            }
            offset += line.len();
        }
        Err(AruError::msg("SKILL.md has unterminated YAML frontmatter"))
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        format!("{}{}", self.frontmatter, self.body).into_bytes()
    }
}

fn protected(key: &str) -> bool {
    matches!(key, "name" | "description")
}

fn yaml(value: &impl Serialize) -> Result<String> {
    serde_yaml_ng::to_string(value)
        .map_err(|error| AruError::msg(format!("could not serialize skill metadata: {error}")))
}

#[cfg(test)]
mod tests;
