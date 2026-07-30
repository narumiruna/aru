use crate::digest::sha256_bytes;
use crate::error::{AruError, Result};
use crate::instruction::DiscoveredInstruction;
use crate::lockfile::{LockedInstructionSource, ProjectionBaseline};
use crate::target::instructions;

pub fn locked_sources(units: &[DiscoveredInstruction]) -> Vec<LockedInstructionSource> {
    let mut output = units
        .iter()
        .map(|instruction| LockedInstructionSource {
            source: instruction.unit.source.to_string_lossy().replace('\\', "/"),
            scope: instruction.unit.scope.clone(),
            targets: instruction.unit.targets.iter().copied().collect(),
            sha256: instruction.unit.source_sha256.clone(),
        })
        .collect::<Vec<_>>();
    output.sort_by(|left, right| left.source.cmp(&right.source));
    output
}

pub fn baselines(units: &[DiscoveredInstruction]) -> Result<Vec<ProjectionBaseline>> {
    let mut output = instructions::render(units)?
        .into_iter()
        .map(|projection| ProjectionBaseline {
            target: projection.target,
            kind: "instruction".into(),
            key: projection.source,
            sha256: sha256_bytes(projection.content.as_bytes()),
        })
        .collect::<Vec<_>>();
    output.sort();
    Ok(output)
}

pub fn validate_locked_sources(
    locked: &[LockedInstructionSource],
    units: &[DiscoveredInstruction],
) -> Result<()> {
    if locked == locked_sources(units) {
        Ok(())
    } else {
        Err(AruError::msg(
            "aru.lock is stale for instruction sources; run aru lock or aru sync",
        ))
    }
}
