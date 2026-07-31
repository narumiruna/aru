use std::path::{Path, PathBuf};

use crate::error::{AruError, Result};
use crate::instruction::{DiscoveredInstruction, InstructionScope};
use crate::manifest::Target;

use super::{InstructionProjection, ProjectionMode, normalized_markdown, quoted};

pub fn render(instruction: &DiscoveredInstruction) -> Result<InstructionProjection> {
    let source = portable(&instruction.unit.source)?;
    match &instruction.unit.scope {
        InstructionScope::SourceDirectory { directory } => {
            let destination = if directory == "." {
                PathBuf::from("CLAUDE.md")
            } else {
                PathBuf::from(directory).join("CLAUDE.md")
            };
            let has_native_projection = instruction.unit.targets.iter().any(|target| {
                crate::target::capabilities(*target).instructions
                    == crate::target::InstructionCapability::NativeAgents
            });
            let content = if instruction.unit.managed && !has_native_projection {
                normalized_markdown(&instruction.content)
            } else {
                "@AGENTS.md\n".into()
            };
            Ok(InstructionProjection {
                target: Target::Claude,
                source,
                destination,
                mode: ProjectionMode::SharedBlock,
                content,
            })
        }
        InstructionScope::ApplyTo { globs } => {
            let destination = PathBuf::from(".claude/rules/aru").join(&instruction.unit.source);
            let mut content = String::from("---\npaths:\n");
            for glob in globs {
                content.push_str("  - ");
                content.push_str(&quoted(glob));
                content.push('\n');
            }
            content.push_str("---\n");
            content.push_str(&normalized_markdown(&instruction.content));
            Ok(InstructionProjection {
                target: Target::Claude,
                source,
                destination,
                mode: ProjectionMode::File,
                content,
            })
        }
    }
}

fn portable(path: &Path) -> Result<String> {
    path.to_str()
        .map(|value| value.replace(std::path::MAIN_SEPARATOR, "/"))
        .ok_or_else(|| AruError::msg("instruction source path is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::digest::sha256_bytes;
    use crate::instruction::{InstructionScope, InstructionUnit};

    use super::*;

    #[test]
    fn directory_anchor_is_a_sibling_import() {
        let instruction = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("src/api/AGENTS.md"),
                scope: InstructionScope::SourceDirectory {
                    directory: "src/api".into(),
                },
                targets: BTreeSet::from([Target::Claude]),
                source_sha256: sha256_bytes(b"ignored"),
                managed: false,
            },
            content: "ignored".into(),
        };
        let projection = render(&instruction).unwrap();
        assert_eq!(projection.destination, PathBuf::from("src/api/CLAUDE.md"));
        assert_eq!(projection.mode, ProjectionMode::SharedBlock);
        assert_eq!(projection.content, "@AGENTS.md\n");
    }

    #[test]
    fn claude_only_managed_package_instruction_embeds_content() {
        let instruction = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("packages/hash/AGENTS.md"),
                scope: InstructionScope::SourceDirectory {
                    directory: ".".into(),
                },
                targets: BTreeSet::from([Target::Claude]),
                source_sha256: sha256_bytes(b"package"),
                managed: true,
            },
            content: "package rules\n".into(),
        };
        let projection = render(&instruction).unwrap();
        assert_eq!(projection.destination, PathBuf::from("CLAUDE.md"));
        assert_eq!(projection.content, "package rules\n");
    }

    #[test]
    fn explicit_globs_render_deterministic_rule_frontmatter() {
        let instruction = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("docs/rust.md"),
                scope: InstructionScope::ApplyTo {
                    globs: vec!["**/*.rs".into(), "crates/**".into()],
                },
                targets: BTreeSet::from([Target::Claude]),
                source_sha256: sha256_bytes(b"rust"),
                managed: false,
            },
            content: "# Rust\r\n\r\nAvoid unwrap.\r\n".into(),
        };
        let projection = render(&instruction).unwrap();
        assert_eq!(
            projection.destination,
            PathBuf::from(".claude/rules/aru/docs/rust.md")
        );
        assert_eq!(
            projection.content,
            include_str!("../../../tests/fixtures/instructions/claude-rust-rule.md")
        );
    }
}
