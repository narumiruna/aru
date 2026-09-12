use std::borrow::Cow;
use std::io::{self, IsTerminal};

use crate::cli::ColorChoice;

mod progress;
pub use progress::Progress;

const SUMMARY_LIMIT: usize = 12;

#[derive(Debug, Clone, Copy)]
pub struct Output {
    quiet: bool,
    verbose: u8,
    color: bool,
    no_progress: bool,
    stderr_terminal: bool,
    interactive: bool,
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
            interactive: false,
        }
    }

    pub fn with_interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub(crate) fn color_enabled(&self) -> bool {
        self.color
    }

    /// Keep this guard alive only around work that does not print or prompt.
    pub fn progress(&self, message: &str) -> Progress {
        self.activity("Resolving", message)
    }

    pub fn applying(&self, message: &str) -> Progress {
        if self.inline_enabled() {
            self.activity("Applying", message)
        } else {
            Progress::hidden()
        }
    }

    fn activity(&self, label: &str, message: &str) -> Progress {
        let supports_animation = std::env::var_os("TERM").is_none_or(|term| term != "dumb");
        match self.progress_mode(supports_animation) {
            ProgressMode::Hidden => Progress::hidden(),
            ProgressMode::Static => {
                self.emit(label, &inline_text(message, 160), "36");
                Progress::hidden()
            }
            ProgressMode::Animated => Progress::start(label, message, self.color),
        }
    }

    fn progress_mode(&self, supports_animation: bool) -> ProgressMode {
        if self.quiet || self.no_progress || !self.stderr_terminal {
            ProgressMode::Hidden
        } else if self.interactive && supports_animation {
            ProgressMode::Animated
        } else {
            ProgressMode::Static
        }
    }

    fn inline_enabled(&self) -> bool {
        self.interactive && self.stderr_terminal && !self.quiet
    }

    pub fn step(&self, message: &str) {
        if self.inline_enabled() {
            self.emit("Step", message, "36");
        }
    }

    /// Informational only: the existing transaction still validates every write.
    pub fn summary(&self, plan: &[String]) {
        if !self.inline_enabled() || plan.is_empty() {
            return;
        }
        self.emit(
            "Plan",
            &format!("{} planned actions; applying next", plan.len()),
            "36",
        );
        for line in summary_lines(plan) {
            self.emit("Would", &line, "36");
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

    pub fn preview(&self, message: &str) {
        if !self.quiet {
            self.emit("Resolved", message, "36");
        }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProgressMode {
    Hidden,
    Static,
    Animated,
}

fn summary_lines(plan: &[String]) -> Vec<String> {
    let mut lines = plan
        .iter()
        .take(SUMMARY_LIMIT)
        .map(|item| inline_text(item, 160))
        .collect::<Vec<_>>();
    if plan.len() > SUMMARY_LIMIT {
        lines.push(format!(
            "... and {} more actions; use --dry-run for the full preview",
            plan.len() - SUMMARY_LIMIT
        ));
    }
    lines
}

// Metadata must not inject terminal controls into transient UI or plan summaries.
fn inline_text(text: &str, limit: usize) -> String {
    let mut chars = text.chars();
    let mut result: String = chars
        .by_ref()
        .take(limit)
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    if chars.next().is_some() {
        result.push_str("...");
    }
    result
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
    fn presentation_modes_respect_terminal_and_output_flags() {
        for terminal in [false, true] {
            for interactive in [false, true] {
                for quiet in [false, true] {
                    for no_progress in [false, true] {
                        let output = Output::with_terminal(
                            quiet,
                            0,
                            ColorChoice::Auto,
                            no_progress,
                            terminal,
                        )
                        .with_interactive(interactive);
                        assert_eq!(output.color, terminal);
                        assert_eq!(output.inline_enabled(), terminal && interactive && !quiet);
                        let expected = if !terminal || quiet || no_progress {
                            ProgressMode::Hidden
                        } else if interactive {
                            ProgressMode::Animated
                        } else {
                            ProgressMode::Static
                        };
                        assert_eq!(output.progress_mode(true), expected);
                        assert_ne!(output.progress_mode(false), ProgressMode::Animated);
                    }
                }
            }
        }
        assert!(!Output::with_terminal(false, 0, ColorChoice::Never, false, true).color);
        assert!(Output::with_terminal(false, 0, ColorChoice::Always, false, false).color);
    }

    #[test]
    fn summaries_are_ordered_bounded_and_control_safe() {
        let plan = (0..20)
            .map(|index| format!("create skill demo-{index}"))
            .collect::<Vec<_>>();
        let lines = summary_lines(&plan);
        assert_eq!(lines.len(), SUMMARY_LIMIT + 1);
        assert_eq!(&lines[..SUMMARY_LIMIT], &plan[..SUMMARY_LIMIT]);
        assert!(lines[SUMMARY_LIMIT].contains("8 more actions"));
        assert!(lines[SUMMARY_LIMIT].contains("--dry-run"));
        assert!(summary_lines(&[]).is_empty());
        assert_eq!(summary_lines(&plan[..2]), plan[..2]);
        assert_eq!(inline_text("bad\x1b[31m\r\nname", 100), "bad [31m  name");
        assert_eq!(inline_text("技能名稱", 2), "技能...");
        assert_eq!(inline_text("abc", 3), "abc");
    }

    #[test]
    fn plan_actions_use_cargo_style_labels() {
        assert_eq!(humanize("create skill demo").0, "Created");
        assert_eq!(humanize("lock skill demo 1.0.0").0, "Locked");
        assert_eq!(humanize("write lockfile").1, "aru.lock");
        assert_eq!(humanize("force replace MCP docs").0, "Replaced");
    }
}
