use std::io::Write;
use std::path::Path;

use crate::cli::{AuditArgs, AuditFormat};
use crate::error::{AruError, IoContext, Result};

use super::ExecutionPolicy;

pub(super) fn run(project: &Path, args: AuditArgs, policy: ExecutionPolicy) -> Result<()> {
    let report = crate::audit::Report::inspect(project);
    let bytes = match args.format {
        AuditFormat::Text => report.text_bytes(),
        AuditFormat::Json => report.json_bytes()?,
    };

    if let Some(path) = args.output {
        std::fs::write(&path, bytes).at(&path)?;
        policy
            .output
            .completion(&format!("Wrote audit report to {}.", path.display()));
    } else {
        match args.format {
            AuditFormat::Text => {
                std::io::stderr().write_all(&bytes).at("stderr")?;
            }
            AuditFormat::Json => {
                std::io::stdout().write_all(&bytes).at("stdout")?;
            }
        }
    }

    if report.has_blocking_findings() {
        let count = report
            .findings()
            .iter()
            .filter(|finding| finding.severity == crate::audit::Severity::Error)
            .count();
        policy.output.completion(&format!(
            "Audit found {count} blocking finding{}.",
            if count == 1 { "" } else { "s" }
        ));
        return Err(AruError::Reported);
    }

    if report.findings().is_empty() {
        policy.output.completion("Audit passed; no findings.");
    } else {
        policy.output.completion(&format!(
            "Audit passed with {} non-blocking finding{}.",
            report.findings().len(),
            if report.findings().len() == 1 {
                ""
            } else {
                "s"
            }
        ));
    }
    Ok(())
}
