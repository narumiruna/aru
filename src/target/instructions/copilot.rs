use std::path::Path;

use crate::error::{AruError, Result};
use crate::instruction::{DiscoveredInstruction, InstructionScope};
use crate::manifest::Target;

use super::{
    InstructionProjection, ProjectionMode, generated_copilot_path, normalized_markdown, quoted,
};

pub fn render(instruction: &DiscoveredInstruction) -> Result<InstructionProjection> {
    let source = portable(&instruction.unit.source)?;
    match &instruction.unit.scope {
        InstructionScope::SourceDirectory { directory } if directory == "." => {
            Ok(InstructionProjection {
                target: Target::Copilot,
                source,
                destination: ".github/copilot-instructions.md".into(),
                mode: ProjectionMode::SharedBlock,
                content: normalized_markdown(&instruction.content),
            })
        }
        InstructionScope::SourceDirectory { directory } => Ok(path_specific(
            instruction,
            source,
            &[format!("{directory}/**")],
        )),
        InstructionScope::ApplyTo { globs } => Ok(path_specific(instruction, source, globs)),
    }
}

fn path_specific(
    instruction: &DiscoveredInstruction,
    source: String,
    globs: &[String],
) -> InstructionProjection {
    let mut content = String::from("---\napplyTo: ");
    content.push_str(&quoted(&globs.join(",")));
    content.push_str("\n---\n");
    content.push_str(&normalized_markdown(&instruction.content));
    InstructionProjection {
        target: Target::Copilot,
        source,
        destination: generated_copilot_path(&instruction.unit.source),
        mode: ProjectionMode::File,
        content,
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
    use std::path::PathBuf;

    use crate::digest::sha256_bytes;
    use crate::instruction::{InstructionScope, InstructionUnit};

    use super::*;

    #[test]
    fn nested_agents_becomes_path_specific_copilot_file() {
        let instruction = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("src/api/AGENTS.md"),
                scope: InstructionScope::SourceDirectory {
                    directory: "src/api".into(),
                },
                targets: BTreeSet::from([Target::Copilot]),
                source_sha256: sha256_bytes(b"api\n"),
                managed: false,
            },
            content: "api\n".into(),
        };
        let rendered = render(&instruction).unwrap();
        assert_eq!(
            rendered.destination,
            PathBuf::from(".github/instructions/aru/src/api/AGENTS.instructions.md")
        );
        assert_eq!(rendered.content, "---\napplyTo: \"src/api/**\"\n---\napi\n");
    }

    #[test]
    fn root_and_nested_content_match_golden_markdown() {
        let root = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("AGENTS.md"),
                scope: InstructionScope::SourceDirectory {
                    directory: ".".into(),
                },
                targets: BTreeSet::from([Target::Copilot]),
                source_sha256: sha256_bytes(b"root"),
                managed: false,
            },
            content: "# Project\r\n\r\nUse the workspace commands.\r\n".into(),
        };
        assert_eq!(
            render(&root).unwrap().content,
            include_str!("../../../tests/fixtures/instructions/copilot-root-block.md")
        );

        let nested = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("src/api/AGENTS.md"),
                scope: InstructionScope::SourceDirectory {
                    directory: "src/api".into(),
                },
                targets: BTreeSet::from([Target::Copilot]),
                source_sha256: sha256_bytes(b"api"),
                managed: false,
            },
            content: "# API\n\nKeep handlers small.\n".into(),
        };
        assert_eq!(
            render(&nested).unwrap().content,
            include_str!("../../../tests/fixtures/instructions/copilot-api-rule.md")
        );
    }
}
