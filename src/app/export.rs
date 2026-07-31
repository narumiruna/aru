use std::io::Write;
use std::path::Path;

use crate::cli::{ExportArgs, ExportFormat};
use crate::error::{AruError, IoContext, Result};
use crate::lockfile::Lockfile;

use super::ExecutionPolicy;

pub(super) fn run(project: &Path, args: ExportArgs, policy: ExecutionPolicy) -> Result<()> {
    let lock = Lockfile::load_optional(project)?
        .ok_or_else(|| AruError::msg("aru export requires an existing aru.lock"))?;
    let bytes = match args.format {
        ExportFormat::CycloneDx15 => {
            crate::export::cyclonedx_1_5(&lock, args.timestamp.as_deref())?
        }
    };
    if let Some(path) = args.output_file {
        std::fs::write(&path, bytes).at(&path)?;
        policy
            .output
            .completion(&format!("Wrote CycloneDX inventory to {}.", path.display()));
    } else {
        std::io::stdout().write_all(&bytes).at("stdout")?;
    }
    Ok(())
}
