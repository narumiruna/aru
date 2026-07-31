use crate::cli::SelfUpdateArgs;
use crate::error::Result;
use crate::output::Output;
use crate::self_update::{UpdateAction, update as apply_update};

pub(super) fn update(args: SelfUpdateArgs, offline: bool, output: Output) -> Result<()> {
    if crate::self_update::is_standalone_build() && !offline {
        output.progress("aru release");
    }
    let outcome = apply_update(args.dry_run, offline)?;
    match outcome.action {
        UpdateAction::UpToDate => output.completion(&format!(
            "aru {} is already up to date.",
            outcome.current_version
        )),
        UpdateAction::LocalNewer => output.completion(&format!(
            "Installed aru {} is newer than latest stable {}; no update performed.",
            outcome.current_version, outcome.latest_version
        )),
        UpdateAction::WouldUpdate => {
            output.plan(
                &format!(
                    "update aru {} -> {} ({})",
                    outcome.current_version,
                    outcome.latest_version,
                    outcome.executable.display()
                ),
                true,
            );
            output.completion("Self-update dry run complete; aru was not changed.");
        }
        UpdateAction::Updated => {
            output.plan(
                &format!(
                    "update aru {} -> {} ({})",
                    outcome.current_version,
                    outcome.latest_version,
                    outcome.executable.display()
                ),
                false,
            );
            output.completion(&format!("aru {} installed.", outcome.latest_version));
        }
    }
    Ok(())
}
