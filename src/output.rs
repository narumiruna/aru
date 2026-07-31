use std::borrow::Cow;
use std::io::{self, IsTerminal};

use crate::cli::ColorChoice;

#[derive(Debug, Clone, Copy)]
pub struct Output {
    quiet: bool,
    verbose: u8,
    color: bool,
    no_progress: bool,
    stderr_terminal: bool,
}

impl Output {
    pub fn new(quiet: bool, verbose: u8, color: ColorChoice, no_progress: bool) -> Self {
        Self::with_terminal(
            quiet,
            verbose,
            color,
            no_progress,
            io::stderr().is_terminal(),
        )
    }

    fn with_terminal(
        quiet: bool,
        verbose: u8,
        color: ColorChoice,
        no_progress: bool,
        stderr_terminal: bool,
    ) -> Self {
        Self {
            quiet,
            verbose,
            color: match color {
                ColorChoice::Auto => stderr_terminal,
                ColorChoice::Always => true,
                ColorChoice::Never => false,
            },
            no_progress,
            stderr_terminal,
        }
    }

    pub fn progress(&self, message: &str) {
        if !self.quiet && !self.no_progress && self.stderr_terminal {
            self.emit("Resolving", message, "36");
        }
    }

    pub fn plan(&self, item: &str, dry_run: bool) {
        if self.quiet {
            return;
        }
        if dry_run {
            self.emit("Would", item, "36");
            return;
        }
        let (label, message) = humanize(item);
        self.emit(label, &message, status_color(label));
    }

    pub fn detail(&self, message: &str) {
        if !self.quiet && self.verbose > 0 {
            self.emit("Detail", message, "2");
        }
    }

    pub fn completion(&self, message: &str) {
        if !self.quiet {
            self.emit("Finished", message, "32");
        }
    }

    pub fn warning(&self, message: &str) {
        self.emit("Warning", message, "33");
    }

    pub fn verbose(&self) -> u8 {
        self.verbose
    }

    fn emit(&self, label: &str, message: &str, color: &str) {
        if self.color {
            eprintln!("\x1b[1;{color}m{label:>12}\x1b[0m {message}");
        } else {
            eprintln!("{label:>12} {message}");
        }
    }
}

fn humanize(item: &str) -> (&'static str, Cow<'_, str>) {
    if item == "write lockfile" {
        return ("Updated", Cow::Borrowed("aru.lock"));
    }
    if item == "write manifest" {
        return ("Updated", Cow::Borrowed("aru.toml"));
    }
    if item == "write local ownership state" {
        return ("Updated", Cow::Borrowed(".aru/state.toml"));
    }
    for (prefix, label) in [
        ("force replace ", "Replaced"),
        ("create ", "Created"),
        ("update ", "Updated"),
        ("remove ", "Removed"),
        ("lock ", "Locked"),
        ("unlock ", "Unlocked"),
        ("refresh ", "Refreshed"),
        ("adopt ", "Adopted"),
        ("forget ", "Forgot"),
        ("add ", "Added"),
    ] {
        if let Some(message) = item.strip_prefix(prefix) {
            return (label, Cow::Borrowed(message));
        }
    }
    ("Changed", Cow::Borrowed(item))
}

fn status_color(label: &str) -> &'static str {
    match label {
        "Removed" | "Unlocked" => "33",
        "Replaced" => "35",
        _ => "32",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_respects_terminal_and_no_progress_modes() {
        let terminal = Output::with_terminal(false, 0, ColorChoice::Auto, false, true);
        assert!(terminal.stderr_terminal && !terminal.no_progress && terminal.color);

        let disabled = Output::with_terminal(false, 0, ColorChoice::Auto, true, true);
        assert!(disabled.no_progress);

        let redirected = Output::with_terminal(false, 0, ColorChoice::Auto, false, false);
        assert!(!redirected.stderr_terminal && !redirected.color);
    }

    #[test]
    fn plan_actions_use_cargo_style_labels() {
        assert_eq!(humanize("create skill demo").0, "Created");
        assert_eq!(humanize("lock skill demo 1.0.0").0, "Locked");
        assert_eq!(humanize("write lockfile").1, "aru.lock");
        assert_eq!(humanize("force replace MCP docs").0, "Replaced");
    }
}
