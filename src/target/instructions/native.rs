use crate::error::Result;
use crate::instruction::DiscoveredInstruction;
use crate::manifest::Target;

use super::InstructionProjection;

pub fn render(
    _target: Target,
    _instruction: &DiscoveredInstruction,
) -> Result<Option<InstructionProjection>> {
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use crate::digest::sha256_bytes;
    use crate::instruction::{InstructionScope, InstructionUnit};

    use super::*;

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
            },
            content: "root\n".into(),
        };
        for target in [Target::Codex, Target::Pi, Target::Opencode] {
            assert!(render(target, &instruction).unwrap().is_none());
        }
    }
}
