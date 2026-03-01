//! Maps [`AgentEvent`] variants to human-readable terminal output.
//!
//! All output follows a simple rule:
//! - LLM text tokens (`TextDelta`) → `print!` to stdout (no newline, streaming)
//! - Tool / progress lines        → `eprintln!` to stderr (avoids interleaving
//!   with streamed tokens on stdout when stdout is piped)
//! - Errors                       → `eprintln!` to stderr
//!
//! `FinalAnswer` emits only a trailing newline; the content was already
//! streamed live as `TextDelta` events.  The REPL layer captures
//! `FinalAnswer.content` for message history — it does not re-display it.

use std::io::{self, Write as _};

use gglib_core::domain::agent::AgentEvent;

use crate::presentation::tables::truncate_string;

// =============================================================================
// Public API
// =============================================================================

/// Render a single [`AgentEvent`] to the terminal.
///
/// `verbose` enables per-iteration progress lines that are suppressed in
/// normal/quiet mode.
pub fn render_event(event: &AgentEvent, verbose: bool) {
    match event {
        AgentEvent::ReasoningDelta { content } => {
            // Chain-of-thought tokens from reasoning models (DeepSeek R1, QwQ, etc.).
            // Printed to stderr so they don't interleave with the answer on stdout.
            eprint!("{content}");
            let _ = io::stderr().flush();
        }

        AgentEvent::TextDelta { content } => {
            print!("{content}");
            // Flush immediately so each token appears as it arrives.
            let _ = io::stdout().flush();
        }

        AgentEvent::ToolCallStart { tool_call } => {
            eprintln!("\n  ⚙   {} …", tool_call.name);
        }

        AgentEvent::ToolCallComplete { result } => {
            let icon = if result.success { "✓" } else { "✗" };
            let preview = truncate_string(&result.content, 80);
            eprintln!("  {icon}  {}ms  {preview}", result.execute_duration_ms);
        }

        AgentEvent::IterationComplete {
            iteration,
            tool_calls,
        } => {
            if verbose {
                eprintln!("  [iter {iteration}, {tool_calls} tool call(s)]");
            }
        }

        AgentEvent::FinalAnswer { .. } => {
            // Content was already printed token-by-token via TextDelta.
            // Emit a trailing newline so the next shell prompt appears cleanly.
            println!();
        }

        AgentEvent::Error { message } => {
            eprintln!("\n  ❌  {message}");
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use gglib_core::domain::agent::{AgentEvent, ToolResult};

    use super::render_event;

    /// Convenience: call render_event and assert it does not panic.
    fn smoke(event: AgentEvent) {
        render_event(&event, false);
        render_event(&event, true);
    }

    #[test]
    fn final_answer_does_not_panic() {
        smoke(AgentEvent::FinalAnswer { content: "42".into() });
    }

    #[test]
    fn error_does_not_panic() {
        smoke(AgentEvent::Error { message: "something went wrong".into() });
    }

    #[test]
    fn iteration_complete_respects_verbose_flag() {
        // verbose=false should suppress the line, verbose=true should print it.
        // Both paths must complete without panicking.
        render_event(&AgentEvent::IterationComplete { iteration: 1, tool_calls: 2 }, false);
        render_event(&AgentEvent::IterationComplete { iteration: 1, tool_calls: 2 }, true);
    }

    #[test]
    fn tool_call_complete_does_not_panic() {
        smoke(AgentEvent::ToolCallComplete {
            result: ToolResult {
                tool_call_id: "c1".into(),
                content: "output".into(),
                success: true,
                wait_ms: 0,
                execute_duration_ms: 5,
            },
        });
    }
}
