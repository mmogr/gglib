#![doc = include_str!("README.md")]
//!
//! ## Redraw strategy: cursor movement, not raw mode
//!
//! Earlier CLI work in this crate (see
//! [`crate::handlers::model::download::run_interactive_monitor`]) already
//! established
//! that `crossterm::terminal::enable_raw_mode()` breaks `println!`-based
//! redraws (it disables `OPOST`, so `\n` stops returning the cursor to column
//! 0). This module never touches raw mode. Instead, each frame after the
//! first moves the cursor up by the previous frame's *physical row* count
//! (see [`visual_row_count`]) and clears everything below before printing
//! the next frame — plain `crossterm::cursor`/`terminal` commands in normal
//! (cooked) mode, which compose fine with ordinary `print!`/`println!`.
//! Cooked mode means a line longer than the terminal's width auto-wraps onto
//! an extra physical row, which is exactly what `visual_row_count` accounts
//! for when computing how far to move the cursor up on the next tick. When
//! stdout is not a TTY (piped output, CI), frames are printed sequentially
//! instead, since there is no cursor to move.
//!
//! ## Shutdown
//!
//! `Ctrl+C` is raced directly against each stream-chunk read via
//! `tokio::select!`, so it is handled between chunks rather than only after a
//! full frame arrives. [`TerminalGuard`] hides the cursor for the duration of
//! the dashboard and unconditionally restores it (and prints a trailing
//! newline) on drop — including on the `Ctrl+C` path, an early `?` return, or
//! a panic — so the terminal is never left in a half-drawn state. Dropping
//! the `reqwest` response stream (which happens automatically once
//! `execute()` returns) closes the underlying SSE connection.

use std::io::{IsTerminal, Write, stdout};

use anyhow::{Context, Result};
use crossterm::{cursor, execute, terminal};
use futures_util::StreamExt;

/// Width (in bar cells) of every progress bar drawn by this dashboard.
const BAR_WIDTH: usize = 20;

/// Fallback terminal width (columns) used when stdout isn't a TTY or
/// `crossterm::terminal::size()` fails to report one. Matches the common
/// default terminal width so output still looks reasonable when piped.
const DEFAULT_TERM_WIDTH: u16 = 80;

mod render;
/// The one section that is not a readback — see the module docs there.
mod render_reasoning;
mod wire;
mod wire_sampling;

use render::{render_frame, visual_row_count};
use wire::DashboardSnapshot;

/// Extract complete SSE `data:` payloads from a growing byte buffer.
///
/// Splits on the blank-line event terminator (`"\n\n"`), joining any
/// `data:`-prefixed lines within an event (gglib-sse always emits single-line
/// JSON, but multi-line `data:` framing is handled per spec anyway). Comment
/// lines (leading `:`, used for SSE keep-alives) and events with no `data:`
/// line are silently skipped. Any trailing partial event is left in `buffer`
/// for the next call once more bytes arrive.
fn drain_sse_events(buffer: &mut String) -> Vec<String> {
    let mut payloads = Vec::new();
    while let Some(idx) = buffer.find("\n\n") {
        let event: String = buffer.drain(..idx + 2).collect();
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if !data.is_empty() {
            payloads.push(data);
        }
    }
    payloads
}

// =============================================================================
// Terminal state guard
// =============================================================================

/// Hides the cursor for the lifetime of the dashboard and unconditionally
/// restores it (plus a trailing newline so the shell prompt doesn't land mid-
/// line) on drop — covering the `Ctrl+C` path, an early `?` return, and an
/// unwinding panic alike. A no-op when stdout isn't a TTY.
struct TerminalGuard {
    is_tty: bool,
}

impl TerminalGuard {
    fn new(is_tty: bool) -> Self {
        if is_tty {
            let _ = execute!(stdout(), cursor::Hide);
        }
        Self { is_tty }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.is_tty {
            let _ = execute!(stdout(), cursor::Show);
            println!();
        }
    }
}

// =============================================================================
// Entry point
// =============================================================================

/// Execute `gglib proxy dashboard`.
///
/// Connects to `http://{host}:{port}/v1/proxy/status/stream`, prints the
/// hydration snapshot immediately, then redraws in place on every subsequent
/// tick until `Ctrl+C` is pressed or the connection is closed by the server.
pub(crate) async fn execute(host: String, port: u16, api_key: Option<&str>) -> Result<()> {
    let url = format!("http://{host}:{port}/v1/proxy/status/stream");

    let mut request = reqwest::Client::new().get(&url);
    if let Some(key) = api_key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("failed to connect to {url} — is the proxy running?"))?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!(
            "the proxy at {host}:{port} requires an API key. Pass --api-key, set \
             GGLIB_API_KEY, or store one with `gglib config settings set --proxy-api-key <key>`."
        );
    }
    if !response.status().is_success() {
        anyhow::bail!(
            "proxy dashboard stream at {url} returned HTTP {}",
            response.status()
        );
    }

    let is_tty = stdout().is_terminal();
    let _terminal_guard = TerminalGuard::new(is_tty);

    let mut byte_stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut previous_frame_lines = 0u16;

    loop {
        tokio::select! {
            // Checked first on every loop iteration (top-to-bottom `select!`
            // polling order) so a pending Ctrl+C is never left behind an
            // in-flight chunk read — instant response as required.
            biased;

            _ = tokio::signal::ctrl_c() => {
                return Ok(());
            }

            chunk = byte_stream.next() => {
                let Some(chunk) = chunk else {
                    // Server closed the connection.
                    return Ok(());
                };
                let chunk = chunk.context("error reading proxy dashboard stream")?;
                buffer.push_str(&String::from_utf8_lossy(&chunk));

                for payload in drain_sse_events(&mut buffer) {
                    let snapshot: DashboardSnapshot = match serde_json::from_str(&payload) {
                        Ok(snapshot) => snapshot,
                        Err(e) => {
                            tracing::debug!("skipping unparseable dashboard event: {e}");
                            continue;
                        }
                    };

                    // Re-check on every tick (not just once) so a mid-session
                    // terminal resize is picked up rather than rendering
                    // against a stale width.
                    let term_width = terminal::size()
                        .map(|(cols, _rows)| cols)
                        .unwrap_or(DEFAULT_TERM_WIDTH);
                    let frame = render_frame(&url, &snapshot, term_width);
                    if is_tty {
                        let mut out = stdout();
                        execute!(
                            out,
                            cursor::MoveUp(previous_frame_lines),
                            terminal::Clear(terminal::ClearType::FromCursorDown)
                        )?;
                        write!(out, "{frame}")?;
                        out.flush()?;
                        previous_frame_lines = visual_row_count(&frame, term_width);
                    } else {
                        print!("{frame}");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_sse_events_extracts_single_complete_event() {
        let mut buffer = String::from("data: {\"a\":1}\n\n");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}"]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn drain_sse_events_leaves_partial_event_buffered() {
        let mut buffer = String::from("data: {\"a\":1}\n\ndata: {\"a\":2}");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}"]);
        assert_eq!(buffer, "data: {\"a\":2}");
    }

    #[test]
    fn drain_sse_events_skips_keepalive_comments() {
        let mut buffer = String::from(": ping\n\ndata: {\"a\":1}\n\n");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}"]);
    }

    #[test]
    fn drain_sse_events_handles_multiple_events_in_one_chunk() {
        let mut buffer = String::from("data: {\"a\":1}\n\ndata: {\"a\":2}\n\n");
        let events = drain_sse_events(&mut buffer);
        assert_eq!(events, vec!["{\"a\":1}", "{\"a\":2}"]);
    }
}
