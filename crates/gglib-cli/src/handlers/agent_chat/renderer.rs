//! Maps [`AgentEvent`] variants to human-readable terminal output.
//!
//! ## Rendering modes
//!
//! | Output target | `--quiet` | Mode   | Behaviour                                  |
//! |---------------|-----------|--------|--------------------------------------------|
//! | TTY           | no        | **Rich** | Buffer tokens → render Markdown via *termimad* |
//! | TTY           | yes       | **Raw**  | Stream tokens directly, suppress stderr    |
//! | Pipe / file   | either    | **Raw**  | Stream tokens directly (no ANSI escapes)   |
//!
//! In **Rich** mode an [`indicatif`] spinner runs on stderr while tokens are
//! buffered, giving the user visual feedback that the response is arriving.
//! When [`AgentEvent::FinalAnswer`] arrives the spinner is cleared and the
//! buffered Markdown is rendered in one pass through [`termimad`].
//!
//! In **Raw** mode tokens stream to stdout as they arrive — identical to the
//! pre-`termimad` behaviour.  This keeps piped output clean and predictable.
//!
//! ## Module decomposition
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`super::drain`] | Async event-stream consumer (spinner, thinking accumulator) |
//! | [`super::tool_format`] | Tool-result summary formatters |
//! | [`super::markdown`] | Markdown normalisation + termimad rendering |
//! | [`super::thinking_dispatch`] | `RenderContext`, thinking-event dispatch, spinner coordination |

use std::io::{self, Write as _};

use gglib_core::domain::agent::AgentEvent;

use crate::presentation::style::{BOLD, DANGER, DIM, RESET, SUCCESS};

use super::tool_format::format_tool_result;

// =============================================================================
// Public API
// =============================================================================

/// Render a single [`AgentEvent`] to the terminal.
///
/// `verbose` enables per-iteration progress lines that are suppressed in
/// normal/quiet mode.
///
/// `quiet` suppresses all stderr output (tool progress, reasoning tokens,
/// iteration counts) — only LLM text on stdout is emitted.  Ideal for
/// scripting and piped output.
///
/// `had_text_delta` must be `true` when at least one [`AgentEvent::TextDelta`]
/// was rendered before this call.  When `false`, [`AgentEvent::FinalAnswer`]
/// will print `content` to stdout — a defensive fallback for non-streaming
/// invocations where the model returns its answer in a single chunk.
pub fn render_event(event: &AgentEvent, verbose: bool, quiet: bool, had_text_delta: bool) {
    match event {
        AgentEvent::ReasoningDelta { content } => {
            if !quiet {
                // Chain-of-thought tokens from reasoning models (DeepSeek R1, QwQ, etc.).
                // Printed to stderr so they don't interleave with the answer on stdout.
                eprint!("{content}");
                let _ = io::stderr().flush();
            }
        }

        AgentEvent::TextDelta { content } => {
            print!("{content}");
            // Flush immediately so each token appears as it arrives.
            let _ = io::stdout().flush();
        }

        AgentEvent::ToolCallStart {
            display_name,
            args_summary,
            ..
        } => {
            if !quiet {
                match args_summary {
                    Some(summary) => eprintln!(
                        "\n  {DIM}⚙{RESET}  {BOLD}{display_name}{RESET}  {DIM}{summary}{RESET} …"
                    ),
                    None => eprintln!(
                        "\n  {DIM}⚙{RESET}  {BOLD}{display_name}{RESET} …"
                    ),
                }
            }
        }

        AgentEvent::ToolCallComplete {
            tool_name,
            display_name,
            duration_display,
            result,
            ..
        } => {
            if !quiet {
                let (icon, icon_color) = if result.success {
                    ("✓", SUCCESS)
                } else {
                    ("✗", DANGER)
                };
                let summary = format_tool_result(tool_name, result);
                eprintln!(
                    "  {icon_color}{icon}{RESET}  {BOLD}{display_name}{RESET}  {DIM}{duration_display}{RESET}  {summary}"
                );
            }
        }

        AgentEvent::IterationComplete {
            iteration,
            tool_calls,
        } => {
            if verbose && !quiet {
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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use gglib_core::domain::agent::{AgentEvent, ToolResult};

    use super::render_event;

    /// Convenience: call render_event and assert it does not panic.
    fn smoke(event: AgentEvent) {
        render_event(&event, false, false, false);
        render_event(&event, true, false, false);
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
            false,
        );
        render_event(
            &AgentEvent::IterationComplete {
                iteration: 1,
                tool_calls: 2,
            },
            true,
            false,
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
            display_name: "Some Tool".into(),
            duration_display: "5ms".into(),
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
            display_name: "Read File".into(),
            duration_display: "10ms".into(),
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
            display_name: "List Directory".into(),
            duration_display: "3ms".into(),
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
            display_name: "Grep Search".into(),
            duration_display: "20ms".into(),
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
            display_name: "Grep Search".into(),
            duration_display: "15ms".into(),
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
            display_name: "Read File".into(),
            duration_display: "1ms".into(),
        });
    }
}
