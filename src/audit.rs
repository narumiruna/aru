use std::collections::BTreeSet;
use std::path::{Component, Path};

use serde::Serialize;
use walkdir::WalkDir;

use crate::error::{AruError, Result};
use crate::lockfile::Lockfile;
use crate::manifest::{Manifest, ManifestDocument};
use crate::ownership::{STATE_FILE, State};
use crate::sync::{ReconcileRequest, prepare_request};
use crate::transaction::JOURNAL_FILE;

const REPORT_VERSION: u32 = 1;
const MAX_SCAN_FILES: usize = 100_000;
const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TEXT_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::Warning => "Warning",
            Self::Info => "Info",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

impl Finding {
    fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            message: message.into(),
            path: None,
            line: None,
            column: None,
            help: None,
        }
    }

    fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    findings: Vec<Finding>,
}

#[derive(Serialize)]
struct JsonReport<'a> {
    version: u32,
    status: &'static str,
    findings: &'a [Finding],
}

impl Report {
    pub fn inspect(project: &Path) -> Self {
        let mut findings = Vec::new();
        if project.join(JOURNAL_FILE).exists() {
            findings.push(
                Finding::error(
                    "transaction.pending",
                    "a recoverable transaction is pending",
                )
                .at(JOURNAL_FILE)
                .help("run a mutating aru command to recover the transaction before continuing"),
            );
        }

        let manifest = match ManifestDocument::load(project) {
            Ok(document) => match document.manifest() {
                Ok(manifest) => Some(manifest),
                Err(error) => {
                    findings.push(
                        Finding::error("manifest.invalid", error.to_string())
                            .at(crate::manifest::MANIFEST_FILE)
                            .help("correct aru.toml and rerun `aru audit`"),
                    );
                    None
                }
            },
            Err(error) => {
                findings.push(
                    Finding::error("manifest.invalid", error.to_string())
                        .at(crate::manifest::MANIFEST_FILE)
                        .help("correct aru.toml and rerun `aru audit`"),
                );
                None
            }
        };

        let lock = match Lockfile::load_optional(project) {
            Ok(Some(lock)) => Some(lock),
            Ok(None) => {
                findings.push(
                    Finding::error("lock.missing", "aru.lock is missing")
                        .at(crate::lockfile::LOCK_FILE)
                        .help("run `aru lock` or `aru sync`"),
                );
                None
            }
            Err(error) => {
                findings.push(
                    Finding::error("lock.invalid", error.to_string())
                        .at(crate::lockfile::LOCK_FILE)
                        .help("regenerate aru.lock from reviewed manifest intent"),
                );
                None
            }
        };

        let state = match State::load(project) {
            Ok(state) => Some(state),
            Err(error) => {
                findings.push(
                    Finding::error("ownership.invalid", error.to_string())
                        .at(STATE_FILE)
                        .help("inspect local ownership state before running a mutating command"),
                );
                None
            }
        };

        if let Some(manifest) = manifest.as_ref() {
            inspect_instruction_content(project, manifest, &mut findings);
        }
        if let Some(state) = state.as_ref() {
            inspect_skill_content(project, state, &mut findings);
        }
        if let Some(lock) = lock.as_ref() {
            inspect_plugin_cache(project, lock, &mut findings);
        }
        if let Some(lock) = lock.as_ref()
            && let Err(error) = crate::export::validate_exportable(lock)
        {
            findings.push(
                Finding::error("export.invalid", error.to_string())
                    .help("regenerate aru.lock from validated project intent"),
            );
        }
        if let (Some(manifest), Some(lock)) = (manifest.as_ref(), lock.as_ref()) {
            inspect_ownership(lock, state.as_ref(), &mut findings);
            inspect_projection(project, manifest, lock, &mut findings);
        }

        findings.sort_by(|left, right| {
            (
                left.severity,
                &left.code,
                &left.path,
                left.line,
                left.column,
                &left.message,
            )
                .cmp(&(
                    right.severity,
                    &right.code,
                    &right.path,
                    right.line,
                    right.column,
                    &right.message,
                ))
        });
        findings.dedup();
        Self { findings }
    }

    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    pub fn has_blocking_findings(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }

    pub fn json_bytes(&self) -> Result<Vec<u8>> {
        let report = JsonReport {
            version: REPORT_VERSION,
            status: if self.has_blocking_findings() {
                "failed"
            } else {
                "passed"
            },
            findings: &self.findings,
        };
        let mut bytes = serde_json::to_vec_pretty(&report)
            .map_err(|error| AruError::msg(format!("could not serialize audit report: {error}")))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn text_bytes(&self) -> Vec<u8> {
        let mut output = String::new();
        for finding in &self.findings {
            let location = match (&finding.path, finding.line, finding.column) {
                (Some(path), Some(line), Some(column)) => format!(" {path}:{line}:{column}"),
                (Some(path), Some(line), None) => format!(" {path}:{line}"),
                (Some(path), None, _) => format!(" {path}"),
                _ => String::new(),
            };
            output.push_str(&format!(
                "{:>12} [{}]{location}: {}\n",
                finding.severity.label(),
                finding.code,
                finding.message
            ));
            if let Some(help) = &finding.help {
                output.push_str(&format!("{:>12} {help}\n", "Help"));
            }
        }
        output.into_bytes()
    }
}

fn inspect_plugin_cache(project: &Path, lock: &Lockfile, findings: &mut Vec<Finding>) {
    let cache = crate::cache::Cache::project(project);
    for plugin in &lock.plugin_packages {
        let Some(checkout) = cache.cached_content(&plugin.source, &plugin.revision) else {
            findings.push(
                Finding::error(
                    "plugin.cache-missing",
                    format!("cached content for plugin {:?} is missing", plugin.name),
                )
                .help("run `aru sync` to restore the verified plugin checkout"),
            );
            continue;
        };
        let root = match crate::plugin::plugin_root(&checkout, plugin.subdir.as_deref()) {
            Ok(root) => root,
            Err(error) => {
                findings.push(Finding::error("plugin.cache-invalid", error.to_string()));
                continue;
            }
        };
        match crate::plugin::inspect_plugin_root(&root, Some(plugin.format)) {
            Ok(inventory) => {
                let manifests = inventory
                    .manifests
                    .iter()
                    .map(|manifest| crate::lockfile::PluginManifestRecord {
                        path: manifest.path.clone(),
                        sha256: manifest.sha256.clone(),
                    })
                    .collect::<Vec<_>>();
                if inventory.tree_sha256 != plugin.tree_sha256 || manifests != plugin.manifests {
                    findings.push(
                        Finding::error(
                            "plugin.cache-drift",
                            format!(
                                "cached content for plugin {:?} does not match aru.lock",
                                plugin.name
                            ),
                        )
                        .help("run `aru sync` to replace the altered immutable cache shard"),
                    );
                }
            }
            Err(error) => findings.push(
                Finding::error(
                    "plugin.cache-invalid",
                    format!("plugin {:?}: {error}", plugin.name),
                )
                .help("run `aru sync` to restore the verified plugin checkout"),
            ),
        }
    }
}

fn inspect_instruction_content(project: &Path, manifest: &Manifest, findings: &mut Vec<Finding>) {
    match crate::instruction::discovery::discover(project, manifest) {
        Ok(instructions) => {
            for instruction in instructions {
                let path = portable(&instruction.unit.source);
                scan_text(&path, &instruction.content, findings);
            }
        }
        Err(error) => findings.push(
            Finding::error("instruction.invalid", error.to_string())
                .help("correct the instruction declaration or source content"),
        ),
    }
}

fn inspect_skill_content(project: &Path, state: &State, findings: &mut Vec<Finding>) {
    let project_root = match project.canonicalize() {
        Ok(path) => path,
        Err(error) => {
            findings.push(Finding::error(
                "content.scan-failed",
                format!("could not canonicalize project root: {error}"),
            ));
            return;
        }
    };
    let mut roots = BTreeSet::new();
    for entry in state.entries.iter().filter(|entry| entry.kind == "skill") {
        let relative = Path::new(&entry.destination);
        if !safe_relative(relative) {
            findings.push(
                Finding::error(
                    "ownership.invalid-destination",
                    format!("unsafe ownership destination {:?}", entry.destination),
                )
                .at(STATE_FILE),
            );
            continue;
        }
        let path = project.join(relative);
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if !canonical.starts_with(&project_root) {
            findings.push(
                Finding::error(
                    "ownership.invalid-destination",
                    format!(
                        "ownership destination {:?} escapes the project",
                        entry.destination
                    ),
                )
                .at(STATE_FILE),
            );
            continue;
        }
        roots.insert(canonical);
    }

    let mut files = 0_usize;
    let mut bytes = 0_u64;
    for root in roots {
        for item in WalkDir::new(&root).follow_links(false).max_depth(64) {
            let item = match item {
                Ok(item) => item,
                Err(error) => {
                    findings.push(Finding::error(
                        "content.scan-failed",
                        format!("skill content scan failed: {error}"),
                    ));
                    break;
                }
            };
            if !item.file_type().is_file() {
                continue;
            }
            files += 1;
            if files > MAX_SCAN_FILES {
                findings.push(Finding::error(
                    "content.scan-limit",
                    format!("skill content scan exceeds {MAX_SCAN_FILES} files"),
                ));
                return;
            }
            let size = match item.metadata() {
                Ok(metadata) => metadata.len(),
                Err(error) => {
                    findings.push(Finding::error(
                        "content.scan-failed",
                        format!("could not inspect {}: {error}", item.path().display()),
                    ));
                    continue;
                }
            };
            bytes = bytes.saturating_add(size);
            if bytes > MAX_SCAN_BYTES {
                findings.push(Finding::error(
                    "content.scan-limit",
                    format!("skill content scan exceeds {MAX_SCAN_BYTES} bytes"),
                ));
                return;
            }
            if size > MAX_TEXT_FILE_BYTES {
                continue;
            }
            let content = match std::fs::read(item.path()) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(content) => content,
                    Err(_) => continue,
                },
                Err(error) => {
                    findings.push(Finding::error(
                        "content.scan-failed",
                        format!("could not read {}: {error}", item.path().display()),
                    ));
                    continue;
                }
            };
            let relative = item
                .path()
                .strip_prefix(&project_root)
                .map(portable)
                .unwrap_or_else(|_| item.path().display().to_string());
            scan_text(&relative, &content, findings);
        }
    }
}

fn inspect_ownership(lock: &Lockfile, state: Option<&State>, findings: &mut Vec<Finding>) {
    let Some(state) = state else {
        return;
    };
    let expected_identity = match lock.lock_identity_digest() {
        Ok(identity) => identity,
        Err(error) => {
            findings.push(Finding::error("lock.invalid", error.to_string()));
            return;
        }
    };
    let desired = lock
        .projection_baselines
        .iter()
        .map(|baseline| (baseline.kind.as_str(), baseline.key.as_str()))
        .collect::<BTreeSet<_>>();
    for entry in &state.entries {
        if entry.lock_identity != expected_identity {
            findings.push(
                Finding::error(
                    "ownership.lock-identity",
                    format!(
                        "ownership entry {:?} does not reference the current lock identity",
                        entry.key
                    ),
                )
                .at(STATE_FILE)
                .help("run `aru sync` after reviewing pending projection changes"),
            );
        }
        if !desired.contains(&(entry.kind.as_str(), entry.key.as_str())) {
            findings.push(
                Finding::error(
                    "ownership.stale-reference",
                    format!(
                        "ownership entry {:?} has no current projection baseline",
                        entry.key
                    ),
                )
                .at(STATE_FILE)
                .help("run `aru sync` to reconcile safely owned stale entries"),
            );
        }
    }
}

fn inspect_projection(
    project: &Path,
    manifest: &Manifest,
    lock: &Lockfile,
    findings: &mut Vec<Finding>,
) {
    match prepare_request(
        project,
        manifest,
        Some(lock),
        ReconcileRequest::check_project(),
    ) {
        Ok(prepared) => {
            for item in prepared.plan {
                findings.push(
                    Finding::error(
                        "projection.drift",
                        format!("project reconciliation would {item}"),
                    )
                    .help("run `aru sync --dry-run`, review the plan, then run `aru sync`"),
                );
            }
            for warning in prepared.warnings {
                findings.push(Finding {
                    code: "projection.unowned".into(),
                    severity: Severity::Warning,
                    message: warning,
                    path: None,
                    line: None,
                    column: None,
                    help: Some("inspect preserved content manually".into()),
                });
            }
        }
        Err(error) => findings.push(
            Finding::error("projection.invalid", error.to_string())
                .help("run `aru sync --dry-run` and inspect the reported invariant"),
        ),
    }
}

fn scan_text(path: &str, content: &str, findings: &mut Vec<Finding>) {
    let mut line = 1_usize;
    let mut column = 1_usize;
    for character in content.chars() {
        if hidden_unicode(character) {
            findings.push(Finding {
                code: "content.hidden-unicode".into(),
                severity: Severity::Error,
                message: format!(
                    "contains hidden Unicode {} ({})",
                    unicode_code(character),
                    unicode_name(character)
                ),
                path: Some(path.into()),
                line: Some(line),
                column: Some(column),
                help: Some("review and remove the hidden format control from the source".into()),
            });
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
}

pub(crate) fn first_hidden_unicode(content: &str) -> Option<(usize, usize, char)> {
    let mut line = 1_usize;
    let mut column = 1_usize;
    for character in content.chars() {
        if hidden_unicode(character) {
            return Some((line, column, character));
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    None
}

pub(crate) fn hidden_unicode(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200b}'
            | '\u{200c}'
            | '\u{200d}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'
            | '\u{202b}'
            | '\u{202c}'
            | '\u{202d}'
            | '\u{202e}'
            | '\u{2060}'
            | '\u{2061}'
            | '\u{2062}'
            | '\u{2063}'
            | '\u{2064}'
            | '\u{2066}'
            | '\u{2067}'
            | '\u{2068}'
            | '\u{2069}'
            | '\u{feff}'
    )
}

fn unicode_code(character: char) -> String {
    format!("U+{:04X}", character as u32)
}

fn unicode_name(character: char) -> &'static str {
    match character {
        '\u{061c}' => "ARABIC LETTER MARK",
        '\u{200b}' => "ZERO WIDTH SPACE",
        '\u{200c}' => "ZERO WIDTH NON-JOINER",
        '\u{200d}' => "ZERO WIDTH JOINER",
        '\u{200e}' => "LEFT-TO-RIGHT MARK",
        '\u{200f}' => "RIGHT-TO-LEFT MARK",
        '\u{202a}' => "LEFT-TO-RIGHT EMBEDDING",
        '\u{202b}' => "RIGHT-TO-LEFT EMBEDDING",
        '\u{202c}' => "POP DIRECTIONAL FORMATTING",
        '\u{202d}' => "LEFT-TO-RIGHT OVERRIDE",
        '\u{202e}' => "RIGHT-TO-LEFT OVERRIDE",
        '\u{2060}' => "WORD JOINER",
        '\u{2061}' => "FUNCTION APPLICATION",
        '\u{2062}' => "INVISIBLE TIMES",
        '\u{2063}' => "INVISIBLE SEPARATOR",
        '\u{2064}' => "INVISIBLE PLUS",
        '\u{2066}' => "LEFT-TO-RIGHT ISOLATE",
        '\u{2067}' => "RIGHT-TO-LEFT ISOLATE",
        '\u{2068}' => "FIRST STRONG ISOLATE",
        '\u{2069}' => "POP DIRECTIONAL ISOLATE",
        '\u{feff}' => "ZERO WIDTH NO-BREAK SPACE",
        _ => "FORMAT CONTROL",
    }
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn portable(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_controls_are_sorted_and_multilingual_content_is_safe() {
        let mut findings = Vec::new();
        scan_text(
            "rules.md",
            "繁體中文 🧭\nright \u{202e}abc\nzero \u{200b}x",
            &mut findings,
        );
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].line, Some(2));
        assert_eq!(findings[0].column, Some(7));
        assert_eq!(findings[1].line, Some(3));
        assert!(findings[0].message.contains("U+202E"));
    }

    #[test]
    fn json_schema_is_versioned_and_deterministic() {
        let mut report = Report {
            findings: vec![Finding::error("z", "last"), Finding::error("a", "first")],
        };
        report
            .findings
            .sort_by(|left, right| left.code.cmp(&right.code));
        let first = report.json_bytes().unwrap();
        let second = report.json_bytes().unwrap();
        assert_eq!(first, second);
        let value: serde_json::Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["status"], "failed");
    }
}
