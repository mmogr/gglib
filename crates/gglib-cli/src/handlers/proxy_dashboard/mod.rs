#![doc = include_str!("README.md")]

use std::io::{IsTerminal, Write, stdout};

use anyhow::{Context, Result};
use crossterm::{cursor, execute, terminal};
use futures_util::StreamExt;
use gglib_core::sse::DataFrames;

/// Width (in bar cells) of every progress bar drawn by this dashboard.
const BAR_WIDTH: usize = 20;

/// Fallback terminal width (columns) used when stdout isn't a TTY or
/// `crossterm::terminal::size()` fails to report one. Matches the common
/// default terminal width so output still looks reasonable when piped.
const DEFAULT_TERM_WIDTH: u16 = 80;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::format_push_string,
    reason = "grandfathered at lint inheritance, #1157"
)]
mod render;
/// The per-model signals, whose rules are about what not to print.
mod render_defects;
/// The one section that is not a readback — see the module docs there.
mod render_reasoning;
mod wire;
mod wire_sampling;

use render::{render_frame, visual_row_count};
use wire::DashboardSnapshot;

/// Take `chunk` off the stream, and return every snapshot it completed. A
/// payload that is not a snapshot is skipped.
fn snapshots(frames: &mut DataFrames, chunk: &[u8]) -> Vec<DashboardSnapshot> {
    frames
        .push(chunk)
        .iter()
        .filter_map(|payload| match serde_json::from_str(payload) {
            Ok(snapshot) => Some(snapshot),
            Err(e) => {
                tracing::debug!("skipping unparseable dashboard event: {e}");
                None
            }
        })
        .collect()
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

    let mut request = gglib_proxy::loopback::client_for(&host).get(&url);
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
    let mut frames = DataFrames::unbounded();
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

                for snapshot in snapshots(&mut frames, &chunk) {
                    // Re-check on every tick (not just once) so a mid-session
                    // terminal resize is picked up rather than rendering
                    // against a stale width.
                    let term_width = terminal::size()
                        .map_or(DEFAULT_TERM_WIDTH, |(cols, _rows)| cols);
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
#[path = "mod_tests.rs"]
mod mod_tests;
