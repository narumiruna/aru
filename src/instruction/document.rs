use std::collections::BTreeSet;

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

use crate::digest::sha256_bytes;
use crate::error::{AruError, Result};

const MARKER_ENCODE: &AsciiSet = &CONTROLS.add(b' ').add(b'%').add(b'<').add(b'>').add(b'"');
const START_PREFIX: &str = "<!-- aru:instruction:start ";
const END_PREFIX: &str = "<!-- aru:instruction:end ";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Unmanaged(String),
    Block { id: String, content: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedDocument {
    parts: Vec<Part>,
}

impl ManagedDocument {
    pub fn parse(text: &str) -> Result<Self> {
        let mut parts = Vec::new();
        let mut unmanaged_start = 0_usize;
        let mut active: Option<(String, usize, usize)> = None;
        let mut seen = BTreeSet::new();
        let mut offset = 0_usize;

        for raw_line in text.split_inclusive('\n') {
            let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
            let line = line.strip_suffix('\r').unwrap_or(line);
            let line_start = offset;
            let line_end = offset + raw_line.len();
            offset = line_end;

            if line.contains(START_PREFIX) {
                let id = parse_marker(line, START_PREFIX)?;
                if active.is_some() {
                    return Err(AruError::msg("nested aru instruction markers are invalid"));
                }
                if !seen.insert(id.clone()) {
                    return Err(AruError::msg(format!(
                        "duplicate aru instruction marker {id:?}"
                    )));
                }
                if unmanaged_start < line_start {
                    parts.push(Part::Unmanaged(text[unmanaged_start..line_start].into()));
                }
                active = Some((id, line_start, line_end));
            } else if line.contains(END_PREFIX) {
                let id = parse_marker(line, END_PREFIX)?;
                let Some((start_id, _block_start, body_start)) = active.take() else {
                    return Err(AruError::msg("aru instruction end marker has no start"));
                };
                if id != start_id {
                    return Err(AruError::msg(format!(
                        "aru instruction marker mismatch: start {start_id:?}, end {id:?}"
                    )));
                }
                parts.push(Part::Block {
                    id,
                    content: text[body_start..line_start].into(),
                });
                unmanaged_start = line_end;
            } else if (line.contains("<!-- aru:instruction:") || line.contains("aru:instruction:"))
                && (line.contains("<!--") || line.contains("-->"))
            {
                return Err(AruError::msg("malformed aru instruction marker"));
            }
        }
        if let Some((id, _, _)) = active {
            return Err(AruError::msg(format!(
                "aru instruction marker {id:?} has no end"
            )));
        }
        if unmanaged_start < text.len() {
            parts.push(Part::Unmanaged(text[unmanaged_start..].into()));
        }
        if parts.is_empty() && !text.is_empty() {
            parts.push(Part::Unmanaged(text.into()));
        }
        Ok(Self { parts })
    }

    pub fn empty() -> Self {
        Self { parts: Vec::new() }
    }

    pub fn has_blocks(&self) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, Part::Block { .. }))
    }

    pub fn has_unmanaged_content(&self) -> bool {
        self.parts.iter().any(|part| match part {
            Part::Unmanaged(content) => !content.trim().is_empty(),
            Part::Block { .. } => false,
        })
    }

    pub fn block_digest(&self, source: &str) -> Option<String> {
        let id = marker_id(source);
        self.parts.iter().find_map(|part| match part {
            Part::Block { id: found, content } if *found == id => Some(semantic_digest(content)),
            _ => None,
        })
    }

    pub fn set_block(&mut self, source: &str, content: &str) {
        let id = marker_id(source);
        let content = normalize_body(content);
        if let Some(Part::Block {
            content: existing, ..
        }) = self
            .parts
            .iter_mut()
            .find(|part| matches!(part, Part::Block { id: found, .. } if *found == id))
        {
            *existing = content;
            return;
        }
        if !self.parts.is_empty() {
            let rendered = self.render();
            let separator = if rendered.ends_with("\n\n") {
                ""
            } else if rendered.ends_with('\n') {
                "\n"
            } else {
                "\n\n"
            };
            if !separator.is_empty() {
                self.parts.push(Part::Unmanaged(separator.into()));
            }
        }
        self.parts.push(Part::Block { id, content });
    }

    pub fn remove_block(&mut self, source: &str) -> bool {
        let id = marker_id(source);
        let before = self.parts.len();
        self.parts
            .retain(|part| !matches!(part, Part::Block { id: found, .. } if *found == id));
        before != self.parts.len()
    }

    pub fn remove_all_content(&mut self) {
        self.parts.clear();
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        for part in &self.parts {
            match part {
                Part::Unmanaged(content) => output.push_str(content),
                Part::Block { id, content } => {
                    output.push_str(START_PREFIX);
                    output.push_str(id);
                    output.push_str(" -->\n");
                    output.push_str(content);
                    output.push_str(END_PREFIX);
                    output.push_str(id);
                    output.push_str(" -->\n");
                }
            }
        }
        output
    }

    pub fn is_effectively_empty(&self) -> bool {
        self.render().trim().is_empty()
    }
}

pub fn semantic_digest(content: &str) -> String {
    sha256_bytes(normalize_body(content).as_bytes())
}

pub fn marker_id(source: &str) -> String {
    utf8_percent_encode(source, MARKER_ENCODE).to_string()
}

fn normalize_body(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    format!("{}\n", normalized.trim_end_matches('\n'))
}

fn parse_marker(line: &str, prefix: &str) -> Result<String> {
    let Some(id) = line
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(" -->"))
    else {
        return Err(AruError::msg("malformed aru instruction marker"));
    };
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(AruError::msg("invalid aru instruction marker identifier"));
    }
    Ok(id.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_update_independently_and_preserve_unmanaged_bytes() {
        let original = "# Manual\n\n<!-- aru:instruction:start AGENTS.md -->\nold\n<!-- aru:instruction:end AGENTS.md -->\n";
        let mut document = ManagedDocument::parse(original).unwrap();
        document.set_block("AGENTS.md", "new\n");
        document.set_block("src/api/AGENTS.md", "api\n");
        let rendered = document.render();
        assert!(rendered.starts_with("# Manual\n\n"));
        assert!(rendered.contains("\nnew\n<!-- aru:instruction:end AGENTS.md -->"));
        assert!(rendered.contains("src/api/AGENTS.md -->\napi\n"));
        assert!(document.remove_block("AGENTS.md"));
        assert!(document.render().contains("# Manual"));
        assert!(document.render().contains("api\n"));
    }

    #[test]
    fn malformed_duplicate_and_nested_markers_fail_closed() {
        for invalid in [
            "<!-- aru:instruction:start a -->\n",
            "<!-- aru:instruction:end a -->\n",
            "<!-- aru:instruction:start a -->\n<!-- aru:instruction:start b -->\n",
            "<!-- aru:instruction:start a -->\nx\n<!-- aru:instruction:end b -->\n",
            "<!-- aru:instruction:start a -->\nx\n<!-- aru:instruction:end a -->\n<!-- aru:instruction:start a -->\ny\n<!-- aru:instruction:end a -->\n",
        ] {
            assert!(ManagedDocument::parse(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn marker_ids_are_stable_and_whitespace_is_not_part_of_semantics() {
        assert_eq!(marker_id("src/api/AGENTS.md"), "src/api/AGENTS.md");
        assert_eq!(marker_id("a b/AGENTS.md"), "a%20b/AGENTS.md");
        assert_eq!(semantic_digest("body"), semantic_digest("body\n\n"));
    }
}
