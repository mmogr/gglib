//! The CLI's console: the one place lines and live progress bars meet.
//!
//! `indicatif` redraws its bars in place, and keeps count of the lines it
//! has drawn. A line written straight to the terminal while bars are live
//! throws that count off and strands old frames in scrollback. So the bars
//! all belong to one [`MultiProgress`], and anything printed while they are
//! up goes through it.

use std::sync::{Arc, Mutex, PoisonError};

use indicatif::{MultiProgress, ProgressBar};

/// The CLI's progress bars and the lines printed around them.
///
/// There is one per CLI process, held on
/// [`CliContext`](crate::bootstrap::CliContext).
pub struct CliConsole {
    multi_progress: Arc<MultiProgress>,
    /// The interactive monitor's `[a] queue another  [q] quit` hint bar, if
    /// one has been registered via [`Self::set_footer`]. When present, new
    /// bars are inserted above it instead of appended, so it stays pinned to
    /// the bottom.
    footer: Mutex<Option<ProgressBar>>,
}

impl CliConsole {
    /// A console drawing to stderr, installed as the process-wide console
    /// hook (see [`gglib_core::telemetry::set_console_hook`]).
    ///
    /// Any `tracing` log line — or other output routed through
    /// [`gglib_core::telemetry::console_println`] — printed while this
    /// console is alive goes through [`MultiProgress::println`] instead of
    /// straight to a stream. That erases the live bars, prints the line,
    /// and redraws atomically.
    ///
    /// The hook captures a `Weak` reference, not a strong one, and never
    /// clears itself on drop. In production there is exactly one console
    /// per CLI process (see `bootstrap()`), alive for the process's whole
    /// life, so this never matters there — but tests construct several.
    /// Clearing the global hook on `Drop` would be wrong: dropping an
    /// *earlier* console after a *later* one has installed its own hook
    /// would wipe out the still-active hook out from under it. `Weak`
    /// sidesteps that: once a console's `MultiProgress` is actually gone,
    /// its hook's `upgrade()` starts returning `None` and that hook falls
    /// back to `eprintln!` forever — equivalent to no hook at all — without
    /// needing to touch (or race) whatever hook is currently installed.
    #[must_use]
    pub fn new() -> Self {
        let console = Self::over(MultiProgress::new());

        let hook_target = Arc::downgrade(&console.multi_progress);
        gglib_core::telemetry::set_console_hook(Arc::new(move |line: &str| {
            match hook_target.upgrade() {
                Some(multi_progress) => print_through(&multi_progress, line),
                None => eprintln!("{line}"),
            }
        }));

        console
    }

    /// A console that draws nothing and installs no hook, for a test.
    #[cfg(test)]
    pub(crate) fn hidden() -> Self {
        Self::over(MultiProgress::with_draw_target(
            indicatif::ProgressDrawTarget::hidden(),
        ))
    }

    fn over(multi_progress: MultiProgress) -> Self {
        Self {
            multi_progress: Arc::new(multi_progress),
            footer: Mutex::new(None),
        }
    }

    /// Return a clone of the shared [`MultiProgress`] handle.
    ///
    /// The interactive monitor uses this to call [`MultiProgress::suspend`]
    /// while prompting for additional model IDs.
    #[must_use]
    pub fn multi_progress(&self) -> Arc<MultiProgress> {
        Arc::clone(&self.multi_progress)
    }

    /// Whether the bars are drawn at all. They are not when stderr is not a
    /// terminal, and progress is then printed as plain lines.
    #[must_use]
    pub fn draws_bars(&self) -> bool {
        !self.multi_progress.is_hidden()
    }

    /// Register the interactive monitor's hint bar as the footer: bars added
    /// after it are inserted above it, keeping it pinned to the bottom.
    pub fn set_footer(&self, bar: &ProgressBar) {
        *self.footer.lock().unwrap_or_else(PoisonError::into_inner) = Some(bar.clone());
    }

    /// Put `bar` among the live bars: above the footer when there is one,
    /// and last otherwise.
    #[must_use]
    pub fn add_bar(&self, bar: ProgressBar) -> ProgressBar {
        let footer = self
            .footer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        match footer {
            Some(footer) => self.multi_progress.insert_before(&footer, bar),
            None => self.multi_progress.add(bar),
        }
    }

    /// Take `bar` off the screen and out of the live bars.
    pub fn remove_bar(&self, bar: &ProgressBar) {
        bar.finish_and_clear();
        self.multi_progress.remove(bar);
    }

    /// Print one line above the live bars.
    pub fn println(&self, line: &str) {
        print_through(&self.multi_progress, line);
    }
}

impl Default for CliConsole {
    fn default() -> Self {
        Self::new()
    }
}

/// Print `line` above the bars of `multi_progress`.
///
/// `MultiProgress::println` silently drops the line when the draw target is
/// hidden (non-terminal stderr) rather than falling back to a plain write —
/// checking first keeps non-TTY output (CI, pipes) intact.
fn print_through(multi_progress: &MultiProgress, line: &str) {
    if multi_progress.is_hidden() || multi_progress.println(line).is_err() {
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A footer and then a bar must not panic or lose the bar — the actual
    /// bottom-pinned ordering is `MultiProgress::insert_before`'s contract
    /// (exercised, not reimplemented, by `add_bar`); `MultiProgress` doesn't
    /// expose line order through its public API for a unit test to assert.
    #[test]
    fn a_bar_added_after_the_footer_is_live() {
        let console = CliConsole::hidden();
        let footer = console.multi_progress().add(ProgressBar::new(0));
        footer.set_message("[a] queue another  [q] quit");
        console.set_footer(&footer);

        let bar = console.add_bar(ProgressBar::new(10));
        bar.set_position(4);

        assert_eq!(bar.position(), 4);
        console.remove_bar(&bar);
        assert!(bar.is_finished());
    }

    #[test]
    fn a_hidden_console_draws_no_bars() {
        assert!(!CliConsole::hidden().draws_bars());
    }

    /// Interleaving `console_println` calls (the path a `tracing::warn!`
    /// during a download takes) with live bar updates from the same
    /// console must not panic — this is the exact scenario the console
    /// hook exists to make safe instead of corrupting the bars.
    #[test]
    fn console_println_during_a_live_bar_does_not_panic() {
        let console = CliConsole::new();
        let bar = console.add_bar(ProgressBar::new(100));
        for i in 0..50 {
            bar.set_position(i);
            gglib_core::telemetry::console_println(&format!("log line {i}"));
            console.println(&format!("line {i}"));
        }
    }

    /// Dropping a console must not panic, hang, or otherwise misbehave —
    /// the hook it installed captures a `Weak`, not a strong `Arc`, so it
    /// degrades to `eprintln!` once this console (and its `MultiProgress`)
    /// are gone rather than holding anything alive or needing explicit
    /// teardown. See the doc comment on `CliConsole::new`.
    #[test]
    fn dropping_the_console_is_fine() {
        let console = CliConsole::new();
        let _bar = console.add_bar(ProgressBar::new(100));
        drop(console);

        // The hook is still installed (nothing clears it), but its `Weak`
        // cannot upgrade — this must fall back to `eprintln!`
        // without panicking.
        gglib_core::telemetry::console_println("after drop");
    }
}
