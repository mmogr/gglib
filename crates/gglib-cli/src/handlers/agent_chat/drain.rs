//! Async event-stream consumer for the agentic chat loop.
//!
//! [`drain_event_stream`] pulls [`AgentEvent`]s from a
//! [`tokio::sync::mpsc::Receiver`], classifies inline thinking tokens via
//! [`ThinkingAccumulator`], and renders each event through the
//! [`super::renderer`] and [`super::thinking_dispatch`] modules.
//!
//! Extracted from `renderer.rs` to separate the stateful async orchestration
//! (spinner lifecycle, thinking accumulator, Rich/Raw mode selection) from
//! the stateless per-event formatting in [`super::renderer::render_event`].

use std::io::{self, IsTerminal, Write as _};

use gglib_core::domain::agent::AgentEvent;
use gglib_core::domain::thinking::ThinkingAccumulator;
use tokio::sync::mpsc;

use super::markdown::render_markdown;
use super::renderer::render_event;
use super::thinking_dispatch::{
    RenderContext, close_thinking, dispatch_thinking_event, suspend_or_run,
};

/// Drain `rx` until the channel closes or a [`AgentEvent::FinalAnswer`]
/// arrives, rendering each event.
///
/// Returns `true` only when the turn completed with a [`AgentEvent::FinalAnswer`]
/// event.  Returns `false` when the channel closes without one (e.g. the loop
/// hit max iterations or stagnated).  Cancellation (Ctrl+C) is handled by the
/// caller via `tokio::select!`; this function has no side effects beyond
/// rendering.
///
/// When stdout is a TTY and `quiet` is `false`, tokens are buffered and
/// rendered through [`termimad`] on completion (Rich mode).  An
/// [`indicatif`] spinner runs on stderr while buffering.  In all other
/// cases tokens stream to stdout as they arrive (Raw mode).
///
/// A [`ThinkingAccumulator`] intercepts `TextDelta` events so that inline
/// `<think>` tags are reclassified: reasoning goes to stderr, content to
/// stdout.
///
/// The caller **must** gate any history update on the return value: history
/// from a failed or incomplete turn must not replace the previous context.
pub async fn drain_event_stream(
    rx: &mut mpsc::Receiver<AgentEvent>,
    verbose: bool,
    quiet: bool,
) -> bool {
    let rich = !quiet && io::stdout().is_terminal();
    let stderr_tty = io::stderr().is_terminal();
    let mut acc = ThinkingAccumulator::new();
    let mut ctx = RenderContext::new(rich, stderr_tty, quiet);
    let mut had_text = false;

    while let Some(event) = rx.recv().await {
        match &event {
            // ── Content tokens ───────────────────────────────────────
            AgentEvent::TextDelta { content } => {
                had_text = true;
                for te in acc.push(content) {
                    dispatch_thinking_event(te, &mut ctx);
                }
            }

            // ── Structured reasoning (already classified by the model) ─
            AgentEvent::ReasoningDelta { content } => {
                if !quiet {
                    use gglib_core::domain::thinking::ThinkingEvent;
                    dispatch_thinking_event(
                        ThinkingEvent::ThinkingDelta(content.clone()),
                        &mut ctx,
                    );
                }
            }

            // ── Turn complete ────────────────────────────────────────
            AgentEvent::FinalAnswer { content } => {
                close_thinking(&mut ctx);

                // Flush any pending thinking accumulator state.
                for te in acc.flush() {
                    dispatch_thinking_event(te, &mut ctx);
                }
                close_thinking(&mut ctx);

                // Stop spinner before rendering.
                if let Some(sp) = ctx.spinner.take() {
                    sp.finish_and_clear();
                }

                // Render the final output.
                if rich {
                    let text = if ctx.buf.is_empty() {
                        content.as_str()
                    } else {
                        &ctx.buf
                    };
                    if !text.is_empty() {
                        render_markdown(text);
                    }
                } else {
                    // Raw mode: defensive fallback for non-streaming responses.
                    if !had_text && !content.is_empty() {
                        print!("{content}");
                        let _ = io::stdout().flush();
                    }
                    println!();
                }

                debug_assert!(
                    rx.try_recv().is_err(),
                    "events after FinalAnswer violate agent protocol"
                );
                return true;
            }

            // ── Tool / progress / error events ───────────────────────
            _ => {
                close_thinking(&mut ctx);
                suspend_or_run(ctx.spinner.as_ref(), || {
                    render_event(&event, verbose, quiet, had_text);
                });
            }
        }
    }

    // Channel closed without a FinalAnswer — the loop ended with an error
    // (max iterations, stagnation, etc.).
    close_thinking(&mut ctx);
    if let Some(sp) = ctx.spinner.take() {
        sp.finish_and_clear();
    }
    if rich && !ctx.buf.is_empty() {
        render_markdown(&ctx.buf);
    }
    false
}
