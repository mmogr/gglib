//! Thinking-event dispatch and spinner coordination for [`super::renderer`].
//!
//! [`RenderContext`] bundles the mutable state shared across the drain loop,
//! and [`dispatch_thinking_event`] maps each [`ThinkingEvent`] to the
//! appropriate output action.  This eliminates the duplicated match block
//! that previously appeared in both the `TextDelta` and `FinalAnswer` arms
//! of `drain_event_stream`.

use std::io::{self, Write as _};

use gglib_core::domain::thinking::ThinkingEvent;
use indicatif::ProgressBar;

use crate::presentation::style;

// =============================================================================
// Render context
// =============================================================================

/// Mutable state carried through the event-drain loop.
///
/// Grouping these fields into a struct keeps function signatures short and
/// makes it easy to pass the entire rendering context to helpers without
/// repeating six parameters.
pub(super) struct RenderContext {
    /// Buffered content tokens (Rich mode only).
    pub buf: String,
    /// Whether we are currently inside a thinking banner.
    pub in_thinking: bool,
    /// Active spinner (Rich mode only, created on first content token).
    pub spinner: Option<ProgressBar>,
    /// Rich mode enabled (TTY + not quiet).
    pub rich: bool,
    /// Whether stderr is a TTY (controls thinking banners).
    pub stderr_tty: bool,
    /// Quiet mode — suppresses all stderr output.
    pub quiet: bool,
}

impl RenderContext {
    pub fn new(rich: bool, stderr_tty: bool, quiet: bool) -> Self {
        Self {
            buf: String::new(),
            in_thinking: false,
            spinner: None,
            rich,
            stderr_tty,
            quiet,
        }
    }
}

// =============================================================================
// Thinking-event dispatch
// =============================================================================

/// Map a single [`ThinkingEvent`] to the appropriate output action.
///
/// Called from both the `TextDelta` and `FinalAnswer`-flush paths,
/// eliminating what was previously a duplicated match block.
pub(super) fn dispatch_thinking_event(te: ThinkingEvent, ctx: &mut RenderContext) {
    match te {
        ThinkingEvent::ThinkingDelta(t) if !ctx.quiet => {
            open_thinking(ctx);
            suspend_eprint(ctx.spinner.as_ref(), &t);
        }
        ThinkingEvent::ThinkingEnd => {
            close_thinking(ctx);
        }
        ThinkingEvent::ContentDelta(c) => {
            close_thinking(ctx);
            if ctx.rich {
                ctx.buf.push_str(&c);
                if ctx.spinner.is_none() {
                    ctx.spinner = Some(style::make_spinner());
                }
                if let Some(sp) = &ctx.spinner {
                    sp.set_message(format!("Receiving\u{2026} ({} bytes)", ctx.buf.len()));
                }
            } else {
                print!("{c}");
                let _ = io::stdout().flush();
            }
        }
        _ => {}
    }
}

// =============================================================================
// Spinner / thinking helpers
// =============================================================================

/// Run a closure, temporarily suspending the spinner (if active) so its
/// progress line does not collide with the output.
pub(super) fn suspend_or_run(spinner: Option<&ProgressBar>, f: impl FnOnce()) {
    if let Some(sp) = spinner {
        sp.suspend(f);
    } else {
        f();
    }
}

/// Print to stderr, temporarily suspending the spinner (if active) so the
/// output does not collide with the progress line.
fn suspend_eprint(spinner: Option<&ProgressBar>, text: &str) {
    suspend_or_run(spinner, || {
        eprint!("{text}");
        let _ = io::stderr().flush();
    });
}

/// Open a "Thinking" banner on stderr if not already in thinking mode.
///
/// The spinner is stopped before printing the banner so that its 80 ms
/// steady-tick does not interleave with per-token `eprint!` calls,
/// which would produce garbled output like `word⠴ Receiving…`.
/// The spinner is recreated automatically on the next `ContentDelta`.
fn open_thinking(ctx: &mut RenderContext) {
    if !ctx.in_thinking && ctx.stderr_tty {
        if let Some(sp) = ctx.spinner.take() {
            sp.finish_and_clear();
        }
        style::print_thinking_banner();
        ctx.in_thinking = true;
    }
}

/// Close an open "Thinking" banner on stderr.
pub(super) fn close_thinking(ctx: &mut RenderContext) {
    if ctx.in_thinking && ctx.stderr_tty {
        suspend_or_run(ctx.spinner.as_ref(), style::print_banner_close);
        ctx.in_thinking = false;
    }
}
