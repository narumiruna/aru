use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

/// Clears transient output on both success and early error returns.
#[must_use = "keep the progress guard alive until the operation finishes"]
pub struct Progress(Option<ProgressBar>);

impl Progress {
    pub(super) fn hidden() -> Self {
        Self(None)
    }

    pub(super) fn start(label: &str, message: &str, color: bool) -> Self {
        let template = if color {
            "\x1b[1;36m{prefix:>12}\x1b[0m {spinner} {wide_msg}"
        } else {
            "{prefix:>12} {spinner} {wide_msg}"
        };
        let style = ProgressStyle::with_template(template)
            .expect("static progress template is valid")
            .tick_strings(&["-", "\\", "|", "/", ""]);
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr_with_hz(10));
        bar.set_style(style);
        bar.set_prefix(label.to_owned());
        bar.set_message(super::inline_text(message, 160));
        bar.tick();
        bar.enable_steady_tick(Duration::from_millis(100));
        Self(Some(bar))
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        if let Some(bar) = &self.0 {
            bar.disable_steady_tick();
            bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guards_finish_on_success_and_error() {
        fn operation(bar: ProgressBar, fail: bool) -> Result<(), ()> {
            let _progress = Progress(Some(bar));
            if fail {
                return Err(());
            }
            Ok(())
        }
        for fail in [false, true] {
            let bar = ProgressBar::hidden();
            let result = operation(bar.clone(), fail);
            assert_eq!(result.is_err(), fail);
            assert!(bar.is_finished());
        }
    }
}
