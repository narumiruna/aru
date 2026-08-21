pub mod claude;
pub mod copilot;
pub mod native;

use std::path::PathBuf;

use crate::error::Result;
use crate::instruction::DiscoveredInstruction;
use crate::manifest::Target;
use crate::target::{InstructionCapability, capabilities};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMode {
    SharedBlock,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionProjection {
    pub target: Target,
    pub source: String,
    pub destination: PathBuf,
    pub mode: ProjectionMode,
    pub content: String,
}

pub fn render(units: &[DiscoveredInstruction]) -> Result<Vec<InstructionProjection>> {
    let mut output = Vec::new();
    for unit in units {
        for target in &unit.unit.targets {
            let projection = match capabilities(*target).instructions {
                Some(InstructionCapability::NativeAgents) => native::render(*target, unit),
                Some(InstructionCapability::Claude) => claude::render(unit).map(Some),
                Some(InstructionCapability::Copilot) => copilot::render(unit).map(Some),
                None => Ok(None),
            }?;
            if let Some(projection) = projection {
                output.push(projection);
            }
        }
    }
    output.sort_by(|left, right| {
        (&left.destination, left.target, &left.source).cmp(&(
            &right.destination,
            right.target,
            &right.source,
        ))
    });
    Ok(output)
}

pub(crate) fn normalized_markdown(content: &str) -> String {
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    format!("{}\n", normalized.trim_end_matches('\n'))
}

pub(crate) fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("strings always serialize")
}

pub(crate) fn generated_copilot_path(source: &std::path::Path) -> PathBuf {
    let mut relative = source.to_path_buf();
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("instruction");
    relative.set_file_name(format!("{stem}.instructions.md"));
    PathBuf::from(".github/instructions/aru").join(relative)
}
