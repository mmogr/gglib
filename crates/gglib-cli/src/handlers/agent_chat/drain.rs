//! Async event-stream consumer for the agentic chat loop.
//!
//! [`drain_event_stream`] pulls [`AgentEvent`]s from a
//! [`tokio::sync::mpsc::Receiver`] and renders each event through the
//! [`super::renderer`] and [`super::thinking_dispatch`] modules.
//!
//! Inline `<think>` reclassification is handled upstream by
//! [`gglib_core::normalize::NormalizingStream`] in the LLM adapter, so this
//! drain only consumes already-typed [`AgentEvent::ReasoningDelta`] /
//! [`AgentEvent::TextDelta`] events.

use std::io::{self, IsTerminal, Write as _};

use gglib_core::domain::agent::AgentEvent;
use tokio::sync::mpsc;

use super::markdown::render_markdown;
use super::renderer::render_event;
use super::thinking_dispatch::{
    RenderContext, close_thinking, emit_content, emit_reasoning, suspend_or_run,
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
/// Reasoning tokens (already classified upstream into
/// [`AgentEvent::ReasoningDelta`]) are routed to stderr through a
/// "Thinking" banner; content tokens go to stdout.
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
    let mut ctx = RenderContext::new(rich, stderr_tty, quiet);
    let mut had_text = false;

    while let Some(event) = rx.recv().await {
        match &event {
            // ── Content tokens ───────────────────────────────────────
            AgentEvent::TextDelta { content } => {
                had_text = true;
                emit_content(&mut ctx, content);
            }

            // ── Structured reasoning (already classified upstream) ───
            AgentEvent::ReasoningDelta { content } => {
                emit_reasoning(&mut ctx, content);
            }

            // ── Turn complete ────────────────────────────────────────
            AgentEvent::FinalAnswer { content } => {
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
