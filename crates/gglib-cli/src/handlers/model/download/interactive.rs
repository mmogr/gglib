//! Interactive download monitor for the CLI.
//!
//! Runs a monitoring loop while the download manager is active, rendering
//! progress bars and — in TTY environments — listening for keypresses so
//! the user can queue additional models without restarting the command.
//!
//! # TTY vs. non-TTY
//!
//! | Environment | Behaviour |
//! |---|---|
//! | TTY (normal terminal) | Raw-mode keypress listener, `[a]` / `[q]` hotkeys |
//! | Non-TTY (CI, pipe)    | Plain 250 ms polling loop; exits when queue empties |
//!
//! # Blocking stdin safety
//!
//! When the user presses `[a]`, stdin must be read synchronously inside
//! `MultiProgress::suspend`, which takes a plain closure. To avoid starving
//! the Tokio executor (and pausing background downloads), the entire suspend
//! block is wrapped in [`tokio::task::block_in_place`], which signals to the
//! runtime that the current thread will block and allows other tasks to
//! migrate to different threads.

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use gglib_core::download::QueueSnapshot;
use gglib_core::ports::DownloadManagerPort;
use gglib_download::CliDownloadEventEmitter;

use crate::utils::input::{prompt_string, prompt_string_with_default};

// ─── Public entry point ──────────────────────────────────────────────────────

/// Run the interactive download monitor.
///
/// Blocks until all queued downloads complete, fail, or are cancelled.
/// Failures encountered during the session are printed to stderr on exit.
///
/// The calling `execute()` handler simply awaits this future — all queue
/// interaction and progress rendering is encapsulated here.
pub async fn run_interactive_monitor(
    downloads: Arc<dyn DownloadManagerPort>,
    emitter: Arc<CliDownloadEventEmitter>,
) -> Result<()> {
    if std::io::stdout().is_terminal() {
        run_tty_monitor(downloads, emitter).await
    } else {
        run_plain_monitor(downloads).await
    }
}

// ─── Non-TTY path ────────────────────────────────────────────────────────────

/// Plain polling loop for non-interactive environments (CI, pipes).
///
/// indicatif degrades gracefully on non-TTY stdout, so progress bars
/// still emit periodic lines. We just poll for completion.
async fn run_plain_monitor(downloads: Arc<dyn DownloadManagerPort>) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    loop {
        interval.tick().await;
        let snapshot = downloads.get_queue_snapshot().await?;
        if is_queue_finished(&snapshot) {
            print_failures(&snapshot);
            return Ok(());
        }
    }
}

// ─── TTY path ────────────────────────────────────────────────────────────────

/// Interactive monitor for TTY environments.
///
/// Enables raw mode so individual keypresses are detected without Enter.
/// If raw mode fails (e.g., the platform doesn't support it), falls back
/// gracefully to [`run_plain_monitor`].
async fn run_tty_monitor(
    downloads: Arc<dyn DownloadManagerPort>,
    emitter: Arc<CliDownloadEventEmitter>,
) -> Result<()> {
    let mp = emitter.multi_progress();

    let mut raw = match RawModeGuard::acquire() {
        Ok(g) => g,
        Err(_) => return run_plain_monitor(downloads).await,
    };

    mp.println("[a] add to queue  [q] quit").ok();

    loop {
        // ── Non-blocking keypress check (zero-timeout poll) ───────────────
        if crossterm::event::poll(Duration::ZERO).unwrap_or(false) {
            match crossterm::event::read() {
                Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('a'),
                    kind: KeyEventKind::Press,
                    ..
                })) => {
                    handle_add_to_queue(&downloads, &mp, &mut raw).await;
                    mp.println("[a] add to queue  [q] quit").ok();
                }

                Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('q'),
                    kind: KeyEventKind::Press,
                    ..
                })) => {
                    downloads.cancel_all().await.ok();
                    break;
                }

                Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers,
                    kind: KeyEventKind::Press,
                    ..
                })) if modifiers.contains(KeyModifiers::CONTROL) => {
                    downloads.cancel_all().await.ok();
                    break;
                }

                _ => {}
            }
        }

        // ── Async yield — lets Tokio run download worker tasks ────────────
        tokio::time::sleep(Duration::from_millis(250)).await;

        // ── Completion check ──────────────────────────────────────────────
        let snapshot = downloads.get_queue_snapshot().await?;
        if is_queue_finished(&snapshot) {
            print_failures(&snapshot);
            break;
        }
    }

    drop(raw);
    Ok(())
}

/// Prompt the user for a new model ID and queue it.
///
/// Disables raw mode while reading stdin, wraps the blocking read in
/// [`tokio::task::block_in_place`] so Tokio can keep running download tasks
/// on other threads, then re-enables raw mode before returning.
async fn handle_add_to_queue(
    downloads: &Arc<dyn DownloadManagerPort>,
    mp: &indicatif::MultiProgress,
    raw: &mut RawModeGuard,
) {
    // Step off raw mode so the terminal echoes characters correctly.
    raw.disable();

    // block_in_place: Tokio moves other tasks away from this thread while
    // we block on stdin, ensuring background downloads keep progressing.
    let prompt_result = tokio::task::block_in_place(|| {
        mp.suspend(|| -> Result<Option<(String, Option<String>)>> {
            let model_id = prompt_string("Model ID")?;
            if model_id.is_empty() {
                return Ok(None);
            }
            let quant_str =
                prompt_string_with_default("Quantization (optional, e.g. Q4_K_M)", None)?;
            let quant = if quant_str.is_empty() {
                None
            } else {
                Some(quant_str)
            };
            Ok(Some((model_id, quant)))
        })
    });

    // Best-effort re-enable; if it fails the plain monitor loop continues.
    let _ = raw.enable();

    let entry = match prompt_result {
        Ok(Some(entry)) => entry,
        Ok(None) => return, // user pressed Enter with no input
        Err(e) => {
            mp.println(format!("✗ Input error: {e}")).ok();
            return;
        }
    };

    let (model_id, quant) = entry;
    match Arc::clone(downloads).queue_smart(model_id, quant).await {
        Ok((pos, shards)) => {
            mp.println(format!("↳ queued at position {pos} ({shards} shard(s))"))
                .ok();
        }
        Err(e) => {
            mp.println(format!("✗ Queue error: {e}")).ok();
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Returns `true` when there are no active or pending downloads.
fn is_queue_finished(snapshot: &QueueSnapshot) -> bool {
    snapshot.active_count == 0 && snapshot.pending_count == 0
}

/// Print any recorded failures to stderr.
fn print_failures(snapshot: &QueueSnapshot) {
    for failure in &snapshot.recent_failures {
        eprintln!("✗ {}: {}", failure.display_name, failure.error);
    }
}

// ─── RawModeGuard ─────────────────────────────────────────────────────────────

/// RAII guard that enables crossterm raw mode on construction and disables it
/// on drop, ensuring the terminal is always restored even if the caller returns
/// early via `?`.
struct RawModeGuard {
    active: bool,
}

impl RawModeGuard {
    /// Enable raw mode and return a guard. Returns an error if the terminal
    /// does not support raw mode.
    fn acquire() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self { active: true })
    }

    /// Disable raw mode and mark the guard as inactive.
    fn disable(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
            self.active = false;
        }
    }

    /// Re-enable raw mode. Returns an error on failure; the guard stays
    /// inactive so `Drop` won't attempt a double-disable.
    fn enable(&mut self) -> Result<()> {
        enable_raw_mode()?;
        self.active = true;
        Ok(())
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        self.disable();
    }
}
