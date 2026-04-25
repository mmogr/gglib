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
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

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

    let _raw = match RawModeGuard::acquire() {
        Ok(g) => g,
        Err(_) => return run_plain_monitor(downloads).await,
    };

    // hint_bar: a static ProgressBar owned by the MultiProgress that holds
    // the [a]/[q] hint text. Using a managed bar (rather than mp.println)
    // keeps indicatif's line-count accurate, so suspend/resume never tears.
    let mut hint_bar: Option<ProgressBar> = None;
    let mut seen_items = false;
    let mut last_item_count: u32 = 0;

    loop {
        // ── Non-blocking keypress check (zero-timeout poll) ───────────────
        if crossterm::event::poll(Duration::ZERO).unwrap_or(false) {
            match crossterm::event::read() {
                Ok(Event::Key(KeyEvent {
                    code: KeyCode::Char('a'),
                    kind: KeyEventKind::Press,
                    ..
                })) => {
                    // Inline input via temporary ProgressBars — no stdin
                    // prompts, so nothing leaks into the scrollback buffer.
                    handle_add_to_queue(&downloads, &mp).await;
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

        // ── Completion / progress check ───────────────────────────────────
        let snapshot = downloads.get_queue_snapshot().await?;
        let item_count = snapshot.active_count + snapshot.pending_count;

        if item_count > 0 {
            seen_items = true;
        }

        // Create the hint bar the first time we see activity, then keep
        // its message in sync with the live queue counts on every tick.
        let hint_msg = build_hint_message(snapshot.active_count, snapshot.pending_count);
        if seen_items && hint_bar.is_none() {
            let style = ProgressStyle::with_template("{msg}")
                .unwrap_or_else(|_| ProgressStyle::default_bar());
            let bar = ProgressBar::new(0);
            bar.set_style(style);
            bar.set_message(hint_msg.clone());
            hint_bar = Some(mp.add(bar));
            last_item_count = item_count;
        } else if let Some(bar) = &hint_bar {
            bar.set_message(hint_msg);
        }

        // Re-anchor hint to the bottom whenever new items appear.
        if item_count > last_item_count {
            if let Some(bar) = &hint_bar {
                mp.remove(bar);
                mp.add(bar.clone());
            }
            last_item_count = item_count;
        }

        // Fast-fail: exit immediately if a failure appeared before we ever
        // saw any activity (instant auth error, missing Python helper, etc.).
        let has_failure = !snapshot.recent_failures.is_empty();

        if (seen_items || has_failure) && is_queue_finished(&snapshot) {
            print_failures(&snapshot);
            break;
        }
    }

    // Clear the hint bar cleanly before restoring the terminal.
    if let Some(bar) = hint_bar {
        bar.finish_and_clear();
    }
    drop(_raw);
    Ok(())
}

/// Prompt the user for a new model ID and queue it.
///
/// Uses [`prompt_in_bar`] for input — the prompt is rendered as a temporary
/// managed `ProgressBar`, so keystrokes never reach stdout and there is no
/// scrollback leakage. The terminal stays in raw mode throughout.
async fn handle_add_to_queue(downloads: &Arc<dyn DownloadManagerPort>, mp: &MultiProgress) {
    // — Model ID prompt —
    let model_id_raw = match prompt_in_bar(mp, "Model ID").await {
        Ok(Some(s)) if !s.is_empty() => s,
        _ => return, // empty input, Esc, or Ctrl-C — silently abort
    };

    // Accept inline `-q` flag so the user can paste a full command-line
    // fragment, e.g. `owner/repo -q Q4_K_M`.
    let (model_id, inline_quant) = parse_inline_quant(&model_id_raw);

    // — Quantization prompt (skipped if `-q` was inline) —
    let quant = if inline_quant.is_some() {
        inline_quant
    } else {
        match prompt_in_bar(mp, "Quantization (optional, e.g. Q4_K_M)").await {
            Ok(Some(s)) if !s.is_empty() => Some(s),
            Ok(_) => None,
            Err(_) => return,
        }
    };

    if let Err(e) = Arc::clone(downloads).queue_smart(model_id, quant).await {
        // Show the error in a transient managed bar so it doesn't leak
        // into the scrollback. Auto-clears after 3 seconds.
        let style =
            ProgressStyle::with_template("{msg}").unwrap_or_else(|_| ProgressStyle::default_bar());
        let bar = mp.add(ProgressBar::new(0));
        bar.set_style(style);
        bar.set_message(format!("✗ Queue error: {e}"));
        tokio::time::sleep(Duration::from_secs(3)).await;
        bar.finish_and_clear();
    }
}

/// Read a single line of user input from a temporary `ProgressBar`.
///
/// The bar is rendered as `{label}: {buffer}_` and updates live as the user
/// types. Reads keystrokes via crossterm in raw mode, so nothing is echoed
/// to stdout and no scrollback artifacts are produced.
///
/// # Returns
/// - `Ok(Some(text))` on Enter (text may be empty if user pressed Enter immediately)
/// - `Ok(None)` on Esc (user cancelled)
/// - `Err(_)` on Ctrl-C (caller should treat as cancel)
async fn prompt_in_bar(mp: &MultiProgress, label: &str) -> Result<Option<String>> {
    let style = ProgressStyle::with_template("{prefix}: {msg}")
        .unwrap_or_else(|_| ProgressStyle::default_bar());
    let bar = mp.add(ProgressBar::new(0));
    bar.set_style(style);
    bar.set_prefix(label.to_string());
    bar.set_message("_".to_string());

    let mut buffer = String::new();

    loop {
        if crossterm::event::poll(Duration::ZERO).unwrap_or(false)
            && let Ok(Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            })) = crossterm::event::read()
        {
            match code {
                KeyCode::Enter => {
                    bar.finish_and_clear();
                    return Ok(Some(buffer.trim().to_string()));
                }
                KeyCode::Esc => {
                    bar.finish_and_clear();
                    return Ok(None);
                }
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    bar.finish_and_clear();
                    return Err(anyhow::anyhow!("input cancelled"));
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    bar.set_message(format!("{buffer}_"));
                }
                KeyCode::Char(c) => {
                    buffer.push(c);
                    bar.set_message(format!("{buffer}_"));
                }
                _ => {}
            }
        }

        // Yield to let Tokio run download tasks; their bars tick via
        // their own enable_steady_tick from indicatif's draw thread.
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────
/// Build the hint bar message including live queue counts.
///
/// Examples:
/// - `[a] queue another  [q] quit  •  1 downloading`
/// - `[a] queue another  [q] quit  •  2 downloading, 1 queued`
fn build_hint_message(active: u32, pending: u32) -> String {
    let mut msg = String::from("[a] queue another  [q] quit");
    if active > 0 || pending > 0 {
        msg.push_str("  •  ");
        if active > 0 {
            msg.push_str(&format!("{active} downloading"));
        }
        if pending > 0 {
            if active > 0 {
                msg.push_str(", ");
            }
            msg.push_str(&format!("{pending} queued"));
        }
    }
    msg
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

// ─── RawModeGuard ─────────────────────────────────────────────────────────────

/// RAII guard that enables crossterm raw mode on construction and disables it
/// on drop, ensuring the terminal is always restored even if the caller returns
/// early via `?`.
struct RawModeGuard;

impl RawModeGuard {
    /// Enable raw mode and return a guard. Returns an error if the terminal
    /// does not support raw mode.
    fn acquire() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}
