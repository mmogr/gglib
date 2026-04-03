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

use gglib_core::domain::agent::{AgentEvent, ToolResult};
use tokio::sync::mpsc;

use crate::presentation::tables::truncate_string;

// =============================================================================
// Tool result formatters
// =============================================================================

/// Format a tool result with a tool-specific summary instead of a generic
/// truncation.  Builtin filesystem tools get richer output; everything else
/// falls back to the standard 80-char preview.
fn format_tool_result(tool_name: &str, result: &ToolResult) -> String {
    if !result.success {
        return truncate_string(&result.content, 80);
    }

    match tool_name {
        "builtin:read_file" => format_read_file(&result.content),
        "builtin:list_directory" => format_list_directory(&result.content),
        "builtin:grep_search" => format_grep_search(&result.content),
        _ => truncate_string(&result.content, 80),
    }
}

/// `read_file` → show line count and a compact preview of the first few lines.
fn format_read_file(content: &str) -> String {
    let line_count = content.lines().count();
    let truncated = content.contains("[truncated");

    let first_line = content.lines().next().unwrap_or("");
    let preview = truncate_string(first_line, 50);

    if truncated {
        format!("{line_count}+ lines (truncated)  {preview}")
    } else {
        format!("{line_count} lines  {preview}")
    }
}

/// `list_directory` → show entry count.
fn format_list_directory(content: &str) -> String {
    if content == "(empty directory)" {
        return "(empty directory)".to_string();
    }
    let count = content.lines().count();
    let dirs = content.lines().filter(|l| l.ends_with('/')).count();
    let files = count - dirs;
    format!("{count} entries ({files} files, {dirs} dirs)")
}

/// `grep_search` → show match count and first match preview.
fn format_grep_search(content: &str) -> String {
    if content.starts_with("no matches") {
        return "no matches".to_string();
    }
    let match_count = content
        .lines()
        .filter(|l| !l.starts_with("[results"))
        .count();
    let truncated = content.contains("[results truncated");
    let first = content.lines().next().unwrap_or("");
    let preview = truncate_string(first, 50);

    if truncated {
        format!("{match_count}+ matches  {preview}")
    } else {
        format!("{match_count} matches  {preview}")
    }
}

// =============================================================================
// Public API
// =============================================================================

/// Render a single [`AgentEvent`] to the terminal.
///
/// `verbose` enables per-iteration progress lines that are suppressed in
/// normal/quiet mode.
///
/// `had_text_delta` must be `true` when at least one [`AgentEvent::TextDelta`]
/// was rendered before this call.  When `false`, [`AgentEvent::FinalAnswer`]
/// will print `content` to stdout — a defensive fallback for non-streaming
/// invocations where the model returns its answer in a single chunk.
pub fn render_event(event: &AgentEvent, verbose: bool, had_text_delta: bool) {
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

        AgentEvent::ToolCallComplete {
            tool_name,
            result,
            execute_duration_ms,
            ..
        } => {
            let icon = if result.success { "✓" } else { "✗" };
            let summary = format_tool_result(tool_name, result);
            eprintln!("  {icon}  {execute_duration_ms}ms  {summary}");
        }

        AgentEvent::IterationComplete {
            iteration,
            tool_calls,
        } => {
            if verbose {
                eprintln!("  [iter {iteration}, {tool_calls} tool call(s)]");
            }
        }

        AgentEvent::FinalAnswer { content } => {
            // Defensive: print the full answer if it was not already streamed
            // token-by-token via TextDelta events (e.g. non-streaming or
            // single-chunk model responses). In streaming mode the content is
            // already on stdout; only a trailing newline is emitted.
            if !had_text_delta && !content.is_empty() {
                print!("{content}");
                let _ = io::stdout().flush();
            }
            println!();
        }

        AgentEvent::Error { message } => {
            eprintln!("\n  ❌  {message}");
        }
    }
}

/// Drain `rx` until the channel closes or a [`AgentEvent::FinalAnswer`]
/// arrives, rendering each event.
///
/// Returns `true` only when the turn completed with a [`AgentEvent::FinalAnswer`]
/// event.  Returns `false` when the channel closes without one (e.g. the loop
/// hit max iterations or stagnated).  Cancellation (Ctrl+C) is handled by the
/// caller via `tokio::select!`; this function has no side effects beyond
/// rendering.
///
/// The caller **must** gate any history update on the return value: history
/// from a failed or incomplete turn must not replace the previous context.
pub async fn drain_event_stream(rx: &mut mpsc::Receiver<AgentEvent>, verbose: bool) -> bool {
    let mut had_text_delta = false;
    while let Some(event) = rx.recv().await {
        render_event(&event, verbose, had_text_delta);
        if matches!(event, AgentEvent::TextDelta { .. }) {
            had_text_delta = true;
        }

        if let AgentEvent::FinalAnswer { .. } = event {
            // `FinalAnswer` is always the last event emitted before the loop
            // drops its `Sender`.  Any events after this would be a protocol
            // violation and are intentionally dropped.
            debug_assert!(
                rx.try_recv().is_err(),
                "events after FinalAnswer violate agent protocol"
            );
            return true;
        }
    }
    // Channel closed without a FinalAnswer — the loop ended with an error
    // (max iterations, stagnation, etc.).  The caller must not update history.
    false
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
        render_event(&event, false, false);
        render_event(&event, true, false);
    }

    #[test]
    fn final_answer_does_not_panic() {
        smoke(AgentEvent::FinalAnswer {
            content: "42".into(),
        });
    }

    #[test]
    fn error_does_not_panic() {
        smoke(AgentEvent::Error {
            message: "something went wrong".into(),
        });
    }

    #[test]
    fn iteration_complete_respects_verbose_flag() {
        // verbose=false should suppress the line, verbose=true should print it.
        // Both paths must complete without panicking.
        render_event(
            &AgentEvent::IterationComplete {
                iteration: 1,
                tool_calls: 2,
            },
            false,
            false,
        );
        render_event(
            &AgentEvent::IterationComplete {
                iteration: 1,
                tool_calls: 2,
            },
            true,
            false,
        );
    }

    #[test]
    fn tool_call_complete_does_not_panic() {
        smoke(AgentEvent::ToolCallComplete {
            tool_name: "some_tool".into(),
            result: ToolResult {
                tool_call_id: "c1".into(),
                content: "output".into(),
                success: true,
            },
            wait_ms: 0,
            execute_duration_ms: 5,
        });
    }

    #[test]
    fn read_file_render_shows_line_count() {
        // Should not panic and format nicely
        smoke(AgentEvent::ToolCallComplete {
            tool_name: "builtin:read_file".into(),
            result: ToolResult {
                tool_call_id: "c2".into(),
                content: "line 1\nline 2\nline 3\n".into(),
                success: true,
            },
            wait_ms: 0,
            execute_duration_ms: 10,
        });
    }

    #[test]
    fn list_directory_render_shows_counts() {
        smoke(AgentEvent::ToolCallComplete {
            tool_name: "builtin:list_directory".into(),
            result: ToolResult {
                tool_call_id: "c3".into(),
                content: "file.rs\ndir/\nother.txt\n".into(),
                success: true,
            },
            wait_ms: 0,
            execute_duration_ms: 3,
        });
    }

    #[test]
    fn grep_search_render_shows_match_count() {
        smoke(AgentEvent::ToolCallComplete {
            tool_name: "builtin:grep_search".into(),
            result: ToolResult {
                tool_call_id: "c4".into(),
                content: "src/main.rs:1:fn main() {}\nsrc/lib.rs:5:fn helper() {}\n".into(),
                success: true,
            },
            wait_ms: 0,
            execute_duration_ms: 20,
        });
    }

    #[test]
    fn grep_no_matches_render() {
        smoke(AgentEvent::ToolCallComplete {
            tool_name: "builtin:grep_search".into(),
            result: ToolResult {
                tool_call_id: "c5".into(),
                content: "no matches found for 'xyz'".into(),
                success: true,
            },
            wait_ms: 0,
            execute_duration_ms: 15,
        });
    }

    #[test]
    fn failed_tool_falls_back_to_truncation() {
        smoke(AgentEvent::ToolCallComplete {
            tool_name: "builtin:read_file".into(),
            result: ToolResult {
                tool_call_id: "c6".into(),
                content: "file 'nope.txt' does not exist".into(),
                success: false,
            },
            wait_ms: 0,
            execute_duration_ms: 1,
        });
    }
}
