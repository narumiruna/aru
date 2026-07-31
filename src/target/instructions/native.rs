use crate::error::Result;
use std::path::PathBuf;

use crate::error::AruError;
use crate::instruction::{DiscoveredInstruction, InstructionScope};
use crate::manifest::Target;

use super::{InstructionProjection, ProjectionMode, normalized_markdown};

pub fn render(
    target: Target,
    instruction: &DiscoveredInstruction,
) -> Result<Option<InstructionProjection>> {
    if !instruction.unit.managed {
        return Ok(None);
    }
    let InstructionScope::SourceDirectory { directory } = &instruction.unit.scope else {
        return Err(AruError::msg(format!(
            "managed package instruction {:?} uses apply-to globs unsupported by {target}",
            instruction.unit.source
        )));
    };
    let destination = if directory == "." {
        PathBuf::from("AGENTS.md")
    } else {
        PathBuf::from(directory).join("AGENTS.md")
    };
    Ok(Some(InstructionProjection {
        target,
        source: instruction.unit.source.to_string_lossy().replace('\\', "/"),
        destination,
        mode: ProjectionMode::SharedBlock,
        content: normalized_markdown(&instruction.content),
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use crate::digest::sha256_bytes;
    use crate::instruction::{InstructionScope, InstructionUnit};

    use super::*;

    #[test]
    fn managed_package_instructions_render_owned_project_blocks() {
        let instruction = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("packages/hash/AGENTS.md"),
                scope: InstructionScope::SourceDirectory {
                    directory: ".".into(),
                },
                targets: BTreeSet::from([Target::Codex]),
                source_sha256: sha256_bytes(b"package"),
                managed: true,
            },
            content: "package\n".into(),
        };
        let projection = render(Target::Codex, &instruction).unwrap().unwrap();
        assert_eq!(projection.destination, PathBuf::from("AGENTS.md"));
        assert_eq!(projection.mode, ProjectionMode::SharedBlock);
        assert_eq!(projection.content, "package\n");
    }

    #[test]
    fn native_targets_generate_no_duplicate_file() {
        let instruction = DiscoveredInstruction {
            unit: InstructionUnit {
                source: PathBuf::from("AGENTS.md"),
                scope: InstructionScope::SourceDirectory {
                    directory: ".".into(),
                },
                targets: BTreeSet::from([Target::Codex, Target::Pi, Target::Opencode]),
                source_sha256: sha256_bytes(b"root"),
                managed: false,
            },
            content: "root\n".into(),
        };
        for target in [Target::Codex, Target::Pi, Target::Opencode] {
            assert!(render(target, &instruction).unwrap().is_none());
        }
    }
}
