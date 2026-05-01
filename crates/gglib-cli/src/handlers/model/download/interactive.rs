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
//! | TTY (normal terminal) | Single-keystroke `[a]` / `[q]` hotkeys via `console::Term` |
//! | Non-TTY (CI, pipe)    | Plain 250 ms polling loop; exits when queue empties |
//!
//! # Why `console`, not `crossterm` raw mode
//!
//! Earlier iterations enabled `crossterm::terminal::enable_raw_mode()` to read
//! single keypresses without Enter. That permanently disables the termios
//! `OPOST` (output post-processing) flag for the duration of raw mode, which
//! means newline characters (`\n`) are no longer translated to `\r\n` on
//! output. `indicatif` emits a bare `\n` between each managed bar to separate
//! them; without `OPOST` the cursor never returns to column 0, so every redraw
//! drifts one column to the right and old frames remain on screen — bars
//! appear to "scroll" instead of redrawing in place. The `crossterm` docs
//! confirm this: *"New line character will not be processed therefore
//! `println!` can't be used"* in raw mode.
//!
//! The `console` crate (same authors as `indicatif`, designed for exactly
//! this scenario) provides `Term::read_key()` which briefly enters cbreak
//! mode for a single byte read and then immediately restores cooked mode —
//! `OPOST` stays enabled, so indicatif's redraws continue to work correctly.
//! `Term::read_line()` reads a full line in cooked mode, perfect for the
//! `[a]` "queue another" prompt when wrapped in `MultiProgress::suspend`.
//!
//! # Reader thread and rendezvous channels
//!
//! Because `Term::read_key()` is blocking, it runs on a dedicated thread
//! (`tokio::task::spawn_blocking`). After every keystroke the reader thread
//! parks on a *command* channel waiting for an explicit `Continue` from the
//! main task. This rendezvous gives the main task a guaranteed window during
//! which stdin is idle — necessary when the user presses `[a]` and we need
//! to call `Term::read_line()` ourselves without racing the reader for
//! input bytes.
//!
//! # Blocking stdin safety
//!
//! When the user presses `[a]`, the line-input read happens inside
//! `MultiProgress::suspend`, which takes a plain closure. To avoid starving
//! the Tokio executor (and pausing background downloads) the suspend block
//! is wrapped in `tokio::task::block_in_place`, which signals to the
//! runtime that the current thread will block and allows other tasks to
//! migrate to different threads.

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use console::{Key, Term};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::mpsc;

use gglib_core::download::QueueSnapshot;
use gglib_core::ports::DownloadManagerPort;
use gglib_download::CliDownloadEventEmitter;

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
///
/// The loop will not exit until it has observed at least one non-empty
/// snapshot (`seen_items`), which prevents a premature exit caused by
/// the Tokio runner task not yet being scheduled when the first poll
/// fires. Fast-fail exits early if `recent_failures` appears before any
/// items were ever seen active (e.g. instant auth error).
async fn run_plain_monitor(downloads: Arc<dyn DownloadManagerPort>) -> Result<()> {
    // Brief initial yield so the async runner task can be scheduled and
    // move the queued item from pending → active before we first poll.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut interval = tokio::time::interval(Duration::from_millis(250));
    let mut seen_items = false;
    loop {
        interval.tick().await;
        let snapshot = downloads.get_queue_snapshot().await?;

        if snapshot.active_count > 0 || snapshot.pending_count > 0 {
            seen_items = true;
        }

        // Fast-fail: a failure appeared before we ever saw activity.
        // This covers instant errors (auth, missing Python helper, etc.).
        let has_failure = !snapshot.recent_failures.is_empty();

        if (seen_items || has_failure) && is_queue_finished(&snapshot) {
            print_failures(&snapshot);
            return Ok(());
        }
    }
}

// ─── TTY path ────────────────────────────────────────────────────────────────

/// Command sent from the main task back to the keystroke reader thread,
/// telling it whether to read another key or exit.
enum ReaderCmd {
    Continue,
    Stop,
}

/// Interactive monitor for TTY environments.
///
/// Spawns a dedicated keystroke reader thread (using `console::Term::read_key`)
/// that ships keys to the main loop over an async mpsc channel, then awaits
/// an explicit `Continue` command before reading the next key. The main loop
/// `select!`s between the key channel, a 250 ms render tick, and Ctrl-C.
async fn run_tty_monitor(
    downloads: Arc<dyn DownloadManagerPort>,
    emitter: Arc<CliDownloadEventEmitter>,
) -> Result<()> {
    let mp = emitter.multi_progress();

    // ── Channels ───────────────────────────────────────────────────────────
    // key_tx/key_rx: reader → main, one slot (rendezvous-ish; reader can
    // queue a single key before blocking on the next cmd recv).
    let (key_tx, mut key_rx) = mpsc::channel::<Key>(1);
    // cmd_tx/cmd_rx: main → reader, capacity 1. Reader blocks on `recv()`
    // after every key send, ensuring stdin is never read by the reader
    // while the main task is doing line input.
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ReaderCmd>(1);

    // ── Spawn the keystroke reader on a dedicated blocking thread ──────────
    let reader_handle = tokio::task::spawn_blocking(move || {
        let term = Term::stdout();
        loop {
            let key = match term.read_key() {
                Ok(k) => k,
                Err(_) => return, // stdin closed or unsupported
            };
            // If the channel is closed, main has exited — bail out.
            if key_tx.blocking_send(key).is_err() {
                return;
            }
            // Park until main tells us what to do next. This is the
            // critical synchronization: while we're parked here, main
            // can safely call `Term::read_line()` on stdin.
            match cmd_rx.blocking_recv() {
                Some(ReaderCmd::Continue) => {}
                Some(ReaderCmd::Stop) | None => return,
            }
        }
    });

    // ── Render state ───────────────────────────────────────────────────────
    let mut hint_bar: Option<ProgressBar> = None;
    let mut seen_items = false;
    let mut last_item_count: u32 = 0;
    // Two-step quit: first `q`/Ctrl-C arms `quitting=true` and lets active
    // downloads drain naturally; the second press calls `cancel_all()`.
    // Auto-exit fires when the queue empties on its own.
    let mut quitting = false;

    let mut tick = tokio::time::interval(Duration::from_millis(250));
    // Skip the initial fire-immediately tick so we don't redraw before
    // the runner task has had a chance to populate the queue.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick.tick().await;

    let result: Result<()> = loop {
        tokio::select! {
            // ── Keystroke from reader thread ───────────────────────────────
            maybe_key = key_rx.recv() => {
                match maybe_key {
                    Some(Key::Char('a')) => {
                        // Reader is now parked on cmd_rx.recv() — stdin is
                        // free for us to call Term::read_line() inside the
                        // suspend block.
                        handle_add_to_queue(&downloads, &emitter, &mp).await;
                        // Tell reader to resume reading keys.
                        let _ = cmd_tx.send(ReaderCmd::Continue).await;
                    }
                    Some(Key::Char('q')) | Some(Key::Escape) => {
                        if quitting {
                            // Second press → force quit.
                            downloads.cancel_all().await.ok();
                            let _ = cmd_tx.send(ReaderCmd::Stop).await;
                            break Ok(());
                        }
                        // First press → arm drain mode and update the hint.
                        quitting = true;
                        if let Some(bar) = &hint_bar {
                            bar.set_message(
                                "Draining... press q again to force quit".to_string(),
                            );
                        }
                        let _ = cmd_tx.send(ReaderCmd::Continue).await;
                    }
                    Some(_) => {
                        // Ignore other keys; tell reader to keep going.
                        let _ = cmd_tx.send(ReaderCmd::Continue).await;
                    }
                    None => {
                        // Reader thread exited unexpectedly — fall through
                        // to plain polling for the rest of the session.
                        break run_plain_monitor(downloads.clone()).await;
                    }
                }
            }

            // ── Ctrl-C from terminal (signal, not a key) ──────────────────
            _ = tokio::signal::ctrl_c() => {
                if quitting {
                    downloads.cancel_all().await.ok();
                    let _ = cmd_tx.send(ReaderCmd::Stop).await;
                    break Ok(());
                }
                quitting = true;
                if let Some(bar) = &hint_bar {
                    bar.set_message(
                        "Draining... press q again to force quit".to_string(),
                    );
                }
            }

            // ── 250 ms render / completion tick ────────────────────────────
            _ = tick.tick() => {
                let snapshot = downloads.get_queue_snapshot().await?;
                let item_count = snapshot.active_count + snapshot.pending_count;

                if item_count > 0 {
                    seen_items = true;
                }

                // Create the hint bar the first time we see activity.
                if seen_items && hint_bar.is_none() {
                    let style = ProgressStyle::with_template("{msg}")
                        .unwrap_or_else(|_| ProgressStyle::default_bar());
                    let bar = ProgressBar::new(0);
                    bar.set_style(style);
                    bar.set_message(build_hint_message(
                        snapshot.active_count,
                        snapshot.pending_count,
                    ));
                    hint_bar = Some(mp.add(bar));
                    last_item_count = item_count;
                }

                // Update live counts in the hint message every tick.
                // While quitting, keep the drain hint pinned so the user
                // doesn't lose the "press q again" instruction.
                if let Some(bar) = &hint_bar {
                    if quitting {
                        bar.set_message(
                            "Draining... press q again to force quit".to_string(),
                        );
                    } else {
                        bar.set_message(build_hint_message(
                            snapshot.active_count,
                            snapshot.pending_count,
                        ));
                    }
                }

                // Re-anchor hint to the bottom whenever new items appear.
                if item_count > last_item_count {
                    if let Some(bar) = &hint_bar {
                        mp.remove(bar);
                        mp.add(bar.clone());
                    }
                    last_item_count = item_count;
                }

                // Fast-fail: exit if a failure appeared before any activity.
                let has_failure = !snapshot.recent_failures.is_empty();
                if (seen_items || has_failure) && is_queue_finished(&snapshot) {
                    print_failures(&snapshot);
                    let _ = cmd_tx.send(ReaderCmd::Stop).await;
                    break Ok(());
                }
            }
        }
    };

    // Clear the hint bar cleanly before returning.
    if let Some(bar) = hint_bar {
        bar.finish_and_clear();
    }
    // Best-effort: wait briefly for the reader thread to exit so the
    // terminal isn't left with a half-read pending. read_key() is blocking,
    // so if we've already sent Stop the reader will exit on the next key —
    // we don't actually wait for it (would block on stdin) and rely on
    // the channel close + process exit to clean up.
    drop(reader_handle);
    result
}

/// Prompt the user (via `console::Term::read_line`) for a new model ID
/// and queue it. Runs entirely in cooked mode inside `MultiProgress::suspend`,
/// so termios `OPOST` stays enabled and indicatif resumes cleanly afterward.
///
/// Steady-tick animation on all active bars is paused for the duration of the
/// prompt. Without this, indicatif's per-bar background ticker can race with
/// `suspend` and emit one last redraw frame just as suspend is clearing the
/// region, leaving that frame stranded in scrollback above the prompt.
async fn handle_add_to_queue(
    downloads: &Arc<dyn DownloadManagerPort>,
    emitter: &Arc<CliDownloadEventEmitter>,
    mp: &indicatif::MultiProgress,
) {
    // Quiesce indicatif's animation threads so suspend has exclusive control.
    emitter.pause_animation();

    // block_in_place: Tokio moves other tasks away from this thread while
    // we block on stdin, ensuring background downloads keep progressing.
    let prompt_result = tokio::task::block_in_place(|| {
        mp.suspend(|| -> Result<Option<(String, Option<String>)>> {
            let term = Term::stdout();

            // Print prompts to the same Term we read from so they line up
            // with the input cursor when bars are suspended.
            term.write_line("Model ID:")?;
            let model_id_raw = term.read_line()?;
            let model_id_raw = model_id_raw.trim();
            if model_id_raw.is_empty() {
                return Ok(None);
            }

            // Accept inline `-q` flag so the user can paste a full
            // command-line fragment, e.g. `owner/repo -q Q4_K_M`.
            let (model_id, inline_quant) = parse_inline_quant(model_id_raw);
            let quant = if inline_quant.is_some() {
                inline_quant
            } else {
                term.write_line("Quantization (optional, e.g. Q4_K_M):")?;
                let q = term.read_line()?;
                let q = q.trim();
                if q.is_empty() {
                    None
                } else {
                    Some(q.to_string())
                }
            };
            Ok(Some((model_id, quant)))
        })
    });

    // Restart steady-tick now that suspend has returned and bars are back.
    emitter.resume_animation();

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
        Ok(_) => {
            mp.println("✓ Queued").ok();
        }
        Err(e) => {
            mp.println(format!("✗ Queue error: {e}")).ok();
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Build the hint bar message including live queue counts.
fn build_hint_message(active: u32, pending: u32) -> String {
    format!("[a] queue another  [q] quit   ({active} active, {pending} queued)")
}

/// Parse an optional inline `-q <quant>` suffix from a raw model ID string.
///
/// Accepts the fragment that a user might copy from a CLI invocation, e.g.
/// `owner/repo -q Q4_K_M`. Splits on the first ` -q ` token (case-sensitive)
/// and returns `(model_id, Some(quantization))`. If no flag is found, returns
/// the original string unchanged and `None`.
fn parse_inline_quant(s: &str) -> (String, Option<String>) {
    if let Some((model, quant)) = s.split_once(" -q ") {
        let model = model.trim().to_string();
        let quant = quant.trim().to_string();
        if !model.is_empty() && !quant.is_empty() {
            return (model, Some(quant));
        }
    }
    (s.trim().to_string(), None)
}

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
