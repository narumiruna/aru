pub mod discovery;
pub mod document;
pub mod lock;
pub mod sync;

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::manifest::Target;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum InstructionScope {
    SourceDirectory { directory: String },
    ApplyTo { globs: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionUnit {
    pub source: PathBuf,
    pub scope: InstructionScope,
    pub targets: BTreeSet<Target>,
    pub source_sha256: String,
    pub managed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredInstruction {
    pub unit: InstructionUnit,
    pub content: String,
}
