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

// =============================================================================
// Public API
// =============================================================================

/// Render a single [`AgentEvent`] to the terminal.
///
/// `verbose` enables per-iteration progress lines that are suppressed in
/// normal/quiet mode.
pub fn render_event(event: &AgentEvent, verbose: bool) {
    match event {
        AgentEvent::TextDelta { content } => {
            print!("{content}");
            // Flush immediately so each token appears as it arrives.
            let _ = io::stdout().flush();
        }

        AgentEvent::Thinking { content } => {
            eprintln!("\n  💭  {}", truncate(content, 80));
        }

        AgentEvent::ToolCallStart { tool_call } => {
            eprintln!("\n  ⚙   {} …", tool_call.name);
        }

        AgentEvent::ToolCallComplete { result } => {
            let icon = if result.success { "✓" } else { "✗" };
            let preview = truncate(&result.content, 80);
            eprintln!("  {icon}  {}ms  {preview}", result.duration_ms);
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
// Private helpers
// =============================================================================

/// Truncate `s` to at most `max_chars` characters, appending `…` if cut.
fn truncate(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_gets_ellipsis() {
        let result = truncate("hello world", 5);
        assert_eq!(result, "hello…");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }
}
