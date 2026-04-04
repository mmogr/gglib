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
//! ## Inline thinking fallback
//!
//! A [`ThinkingAccumulator`] intercepts `TextDelta` events so that models
//! emitting inline `<think>` tags (when `--reasoning-format none` or no
//! format was specified) have their reasoning content redirected to stderr
//! and only the answer text reaches stdout.

use std::io::{self, IsTerminal, Write as _};
use std::time::Duration;

use gglib_core::domain::agent::{AgentEvent, ToolResult};
use gglib_core::domain::thinking::{ThinkingAccumulator, ThinkingEvent};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::mpsc;

use crate::presentation::style;

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
// Rich-mode helpers
// =============================================================================

/// Print to stderr, temporarily suspending the spinner (if active) so the
/// output does not collide with the progress line.
fn suspend_eprint(spinner: Option<&ProgressBar>, text: &str) {
    if let Some(sp) = spinner {
        sp.suspend(|| {
            eprint!("{text}");
            let _ = io::stderr().flush();
        });
    } else {
        eprint!("{text}");
        let _ = io::stderr().flush();
    }
}

/// Create a new spinner on stderr for the buffering phase.
fn make_spinner() -> ProgressBar {
    let sp = ProgressBar::new_spinner();
    sp.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} {msg}")
            .expect("valid spinner template"),
    );
    sp.enable_steady_tick(Duration::from_millis(80));
    sp.set_message("Receiving…");
    sp
}

/// Render a Markdown string to stdout through [`termimad`].
fn render_markdown(text: &str) {
    let skin = style::get_markdown_skin();
    print!("{}", skin.term_text(text));
}

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

        AgentEvent::ToolCallStart { tool_call } => {
            if !quiet {
                eprintln!("\n  ⚙   {} …", tool_call.name);
            }
        }

        AgentEvent::ToolCallComplete {
            tool_name,
            result,
            execute_duration_ms,
            ..
        } => {
            if !quiet {
                let icon = if result.success { "✓" } else { "✗" };
                let summary = format_tool_result(tool_name, result);
                eprintln!("  {icon}  {execute_duration_ms}ms  {summary}");
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
    let mut buf = String::new();
    let mut had_text = false;
    let mut in_thinking = false;
    let mut spinner: Option<ProgressBar> = None;

    while let Some(event) = rx.recv().await {
        match &event {
            // ── Content tokens ───────────────────────────────────────
            AgentEvent::TextDelta { content } => {
                had_text = true;
                for te in acc.push(content) {
                    match te {
                        ThinkingEvent::ThinkingDelta(t) if !quiet => {
                            if !in_thinking && stderr_tty {
                                if let Some(sp) = &spinner {
                                    sp.suspend(style::print_thinking_banner);
                                } else {
                                    style::print_thinking_banner();
                                }
                                in_thinking = true;
                            }
                            suspend_eprint(spinner.as_ref(), &t);
                        }
                        ThinkingEvent::ThinkingEnd => {
                            if in_thinking && stderr_tty {
                                if let Some(sp) = &spinner {
                                    sp.suspend(style::print_banner_close);
                                } else {
                                    style::print_banner_close();
                                }
                                in_thinking = false;
                            }
                        }
                        ThinkingEvent::ContentDelta(c) => {
                            if in_thinking && stderr_tty {
                                if let Some(sp) = &spinner {
                                    sp.suspend(style::print_banner_close);
                                } else {
                                    style::print_banner_close();
                                }
                                in_thinking = false;
                            }
                            if rich {
                                buf.push_str(&c);
                                if spinner.is_none() {
                                    spinner = Some(make_spinner());
                                }
                                if let Some(sp) = &spinner {
                                    sp.set_message(format!(
                                        "Receiving\u{2026} ({} bytes)",
                                        buf.len()
                                    ));
                                }
                            } else {
                                print!("{c}");
                                let _ = io::stdout().flush();
                            }
                        }
                        _ => {}
                    }
                }
            }

            // ── Structured reasoning (already classified by the model) ─
            AgentEvent::ReasoningDelta { content } => {
                if !quiet {
                    if !in_thinking && stderr_tty {
                        if let Some(sp) = &spinner {
                            sp.suspend(style::print_thinking_banner);
                        } else {
                            style::print_thinking_banner();
                        }
                        in_thinking = true;
                    }
                    suspend_eprint(spinner.as_ref(), content);
                }
            }

            // ── Turn complete ────────────────────────────────────────
            AgentEvent::FinalAnswer { content } => {
                // Close any open thinking block.
                if in_thinking && stderr_tty {
                    if let Some(sp) = &spinner {
                        sp.suspend(style::print_banner_close);
                    } else {
                        style::print_banner_close();
                    }
                    in_thinking = false;
                }

                // Flush any pending thinking accumulator state.
                for te in acc.flush() {
                    match te {
                        ThinkingEvent::ThinkingDelta(t) if !quiet => {
                            if !in_thinking && stderr_tty {
                                if let Some(sp) = &spinner {
                                    sp.suspend(style::print_thinking_banner);
                                } else {
                                    style::print_thinking_banner();
                                }
                                in_thinking = true;
                            }
                            suspend_eprint(spinner.as_ref(), &t);
                        }
                        ThinkingEvent::ContentDelta(c) if rich => {
                            if in_thinking && stderr_tty {
                                if let Some(sp) = &spinner {
                                    sp.suspend(style::print_banner_close);
                                } else {
                                    style::print_banner_close();
                                }
                                in_thinking = false;
                            }
                            buf.push_str(&c);
                        }
                        ThinkingEvent::ContentDelta(c) => {
                            if in_thinking && stderr_tty {
                                if let Some(sp) = &spinner {
                                    sp.suspend(style::print_banner_close);
                                } else {
                                    style::print_banner_close();
                                }
                                in_thinking = false;
                            }
                            print!("{c}");
                            let _ = io::stdout().flush();
                        }
                        _ => {}
                    }
                }

                // Close thinking if flush produced more thinking content.
                if in_thinking && stderr_tty {
                    if let Some(sp) = &spinner {
                        sp.suspend(style::print_banner_close);
                    } else {
                        style::print_banner_close();
                    }
                }

                // Stop spinner before rendering.
                if let Some(sp) = spinner.take() {
                    sp.finish_and_clear();
                }

                // Render the final output.
                if rich {
                    let text = if buf.is_empty() {
                        content.as_str()
                    } else {
                        &buf
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
                // Close any open thinking block so DIM doesn't leak.
                if in_thinking && stderr_tty {
                    if let Some(sp) = &spinner {
                        sp.suspend(style::print_banner_close);
                    } else {
                        style::print_banner_close();
                    }
                    in_thinking = false;
                }
                if let Some(sp) = &spinner {
                    sp.suspend(|| render_event(&event, verbose, quiet, had_text));
                } else {
                    render_event(&event, verbose, quiet, had_text);
                }
            }
        }
    }

    // Channel closed without a FinalAnswer — the loop ended with an error
    // (max iterations, stagnation, etc.).
    if in_thinking && stderr_tty {
        style::print_banner_close();
    }
    if let Some(sp) = spinner.take() {
        sp.finish_and_clear();
    }
    if rich && !buf.is_empty() {
        render_markdown(&buf);
    }
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
