//! CLI download event emitter.
//!
//! Implements [`DownloadEventEmitterPort`] using `indicatif` [`MultiProgress`] bars,
//! rendering live download progress in the terminal. This is the concrete emitter
//! wired into the download manager when running as a CLI process.
//!
//! A shared [`Arc<MultiProgress>`] handle is exposed so the interactive monitor
//! can call [`MultiProgress::suspend`] while prompting for user input.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressState, ProgressStyle};

use gglib_core::download::{DownloadEvent, format_duration, format_rate};
use gglib_core::events::AppEvent;
use gglib_core::ports::{AppEventEmitter, DownloadEventEmitterPort};

// ─── Style constants ─────────────────────────────────────────────────────────

// A single unified template is used for the entire lifetime of the bar.
//
// Earlier iterations switched between a spinner-only template and a bar
// template once the first `ShardProgress` event arrived with a known total
// size. That `set_style` call could race with the steady-tick redraw and
// indicatif's draw-target line tracking, leaving the spinner frame stranded
// in scrollback while the new template drew fresh one line below it.
//
// The unified template renders fine even when the bar's length is unknown
// (the bar widget appears empty until `set_length` is called with a real
// total) and avoids the mid-stream style switch entirely.
//
// Note the absence of `{bytes_per_sec}` and `{eta}`. Those are indicatif's own
// estimates, derived from the `set_position` calls we make; using them meant
// the CLI ignored the rate the manager had already computed and displayed a
// different number from the GUI for the same transfer. Rate and ETA now arrive
// on the event and are rendered into `{wide_msg}`. `{bytes}` stays — that's a
// size, which indicatif labels correctly as binary (MiB/GiB). `{total_bytes}`
// is overridden below by a custom key (see `total_bytes_key`) rather than left
// as indicatif's builtin: the builtin renders the *length*, and a bar created
// before its total is known has no length rather than a zero one — see the
// `ProgressBar::no_length()` comment on `DownloadStarted` below.
const BAR_TEMPLATE: &str = "{spinner:.cyan} {wide_msg} [{bar:30.cyan/blue}] {bytes}/{total_bytes}";
const TICK_INTERVAL: Duration = Duration::from_millis(120);

// ─── CliDownloadEventEmitter ─────────────────────────────────────────────────

/// Terminal-rendering download event emitter for CLI contexts.
///
/// Wraps an `indicatif` [`MultiProgress`] and maintains one [`ProgressBar`] per
/// active download, keyed by the canonical download ID string. Events emitted by
/// the [`DownloadManagerPort`] are translated into bar updates synchronously (the
/// `emit` method does not block on I/O).
///
/// The inner [`MultiProgress`] is shared via [`Arc`] so the interactive monitor
/// can suspend rendering while collecting user input without tearing the display.
pub struct CliDownloadEventEmitter {
    multi_progress: Arc<MultiProgress>,
    bars: Mutex<HashMap<String, ProgressBar>>,
    /// The interactive monitor's `[a] queue another  [q] quit` hint bar, if
    /// one has been registered via [`Self::set_footer`]. When present, new
    /// download bars are inserted above it instead of appended, so it stays
    /// pinned to the bottom without the remove-then-re-add churn that used
    /// to re-anchor it on every new item.
    footer: Mutex<Option<ProgressBar>>,
}

impl CliDownloadEventEmitter {
    /// Create a new emitter backed by a fresh [`MultiProgress`], and install
    /// it as the process-wide console hook (see
    /// [`gglib_core::telemetry::set_console_hook`]).
    ///
    /// Any `tracing` log line — or other output routed through
    /// [`gglib_core::telemetry::console_println`] — printed while this
    /// emitter is alive goes through [`MultiProgress::println`] instead of
    /// straight to a stream. That erases the live bars, prints the line,
    /// and redraws atomically, so the bars' internal line-count bookkeeping
    /// never falls out of sync with what's actually on screen. A raw
    /// `eprintln!`/`println!` racing the bars' own redraws is exactly what
    /// stranded old frames in scrollback before this was wired up.
    #[must_use]
    pub fn new() -> Self {
        let multi_progress = Arc::new(MultiProgress::new());

        let hook_target = Arc::clone(&multi_progress);
        gglib_core::telemetry::set_console_hook(Arc::new(move |line: &str| {
            // `MultiProgress::println` silently drops the line when the draw
            // target is hidden (non-terminal stderr) rather than falling
            // back to a plain write — checking first keeps non-TTY output
            // (CI, pipes) intact.
            if hook_target.is_hidden() || hook_target.println(line).is_err() {
                eprintln!("{line}");
            }
        }));

        Self {
            multi_progress,
            bars: Mutex::new(HashMap::new()),
            footer: Mutex::new(None),
        }
    }

    /// Register the interactive monitor's hint bar as the footer: subsequent
    /// download bars are inserted above it via `MultiProgress::insert_before`
    /// instead of appended, keeping it pinned to the bottom.
    pub fn set_footer(&self, bar: &ProgressBar) {
        if let Ok(mut footer) = self.footer.lock() {
            *footer = Some(bar.clone());
        }
    }

    /// Return a clone of the shared [`MultiProgress`] handle.
    ///
    /// The interactive monitor uses this to call [`MultiProgress::suspend`]
    /// while prompting for additional model IDs.
    #[must_use]
    pub fn multi_progress(&self) -> Arc<MultiProgress> {
        Arc::clone(&self.multi_progress)
    }

    /// Pause the steady-tick animation thread on every active bar.
    ///
    /// indicatif's `enable_steady_tick` spawns a background thread that
    /// redraws the bar at a fixed interval to keep spinners animating.
    /// That thread can race with [`MultiProgress::suspend`]: if it fires
    /// in the narrow window between suspend acquiring its draw lock and
    /// the user's prompt closure starting, the extra frame can be left in
    /// scrollback as a visual artifact above the prompts. Callers that are
    /// about to suspend the display for blocking input should call this
    /// first and [`Self::resume_animation`] when done.
    pub fn pause_animation(&self) {
        if let Ok(bars) = self.bars.lock() {
            for bar in bars.values() {
                bar.disable_steady_tick();
            }
        }
    }

    /// Re-enable the steady-tick animation thread on every active bar.
    ///
    /// Pairs with [`Self::pause_animation`].
    pub fn resume_animation(&self) {
        if let Ok(bars) = self.bars.lock() {
            for bar in bars.values() {
                bar.enable_steady_tick(TICK_INTERVAL);
            }
        }
    }

    /// Create and register the bar for a newly started download, labeled
    /// `label`.
    ///
    /// Uses the unified [`BAR_TEMPLATE`] — see its module-level comment for
    /// why the style never switches mid-stream — and [`ProgressBar::no_length`]
    /// rather than `new(0)`: indicatif's `fraction()` treats an explicit
    /// length of 0 as 100% complete, so a bar created before its total is
    /// known would render fully filled — exactly backwards — until the first
    /// Progress/ShardProgress event calls `set_length`. No length at all
    /// renders as an honestly empty bar instead.
    ///
    /// Inserted above the footer (if [`Self::set_footer`] has registered
    /// one) rather than appended, so the `[a]/[q]` hint bar stays pinned to
    /// the bottom without needing to be removed and re-added.
    fn start_bar(&self, id: String, label: String) {
        let style = ProgressStyle::with_template(BAR_TEMPLATE)
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("█▓░")
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .with_key("total_bytes", total_bytes_key);

        let footer = self.footer.lock().ok().and_then(|f| f.clone());
        let bar = footer.as_ref().map_or_else(
            || self.multi_progress.add(ProgressBar::no_length()),
            |footer| {
                self.multi_progress
                    .insert_before(footer, ProgressBar::no_length())
            },
        );
        bar.set_style(style);
        bar.enable_steady_tick(TICK_INTERVAL);
        bar.set_message(label);

        if let Ok(mut bars) = self.bars.lock() {
            bars.insert(id, bar);
        }
    }

    /// Apply a progress update to the bar registered for `id`.
    ///
    /// Shared by the sharded and unsharded progress arms — they differ only in
    /// which byte counts they report and how they label themselves.
    fn update_bar(&self, id: &str, position: u64, total: u64, message: &str) {
        let Ok(bars) = self.bars.lock() else {
            return;
        };
        let Some(bar) = bars.get(id) else {
            return;
        };

        // Adopt the total whenever it changes — for a shard group the
        // aggregate total is refined as shard sizes resolve. No `set_style`
        // here; the unified template is already in place from DownloadStarted.
        if total > 0 && bar.length().unwrap_or(0) != total {
            bar.set_length(total);
        }
        bar.set_position(position);
        bar.set_message(message.to_string());
    }
}

/// Custom `{total_bytes}` renderer: `—` while the length is unknown, the
/// human-readable size once `set_length` has been called with a real total.
/// Registered via `ProgressStyle::with_key` — see the module-level comment on
/// `BAR_TEMPLATE`.
fn total_bytes_key(state: &ProgressState, w: &mut dyn std::fmt::Write) {
    match state.len() {
        Some(len) => {
            let _ = write!(w, "{}", HumanBytes(len));
        }
        None => {
            let _ = write!(w, "—");
        }
    }
}

/// Render the rate and ETA the manager computed, e.g. `118.4 MB/s · ETA 2m 40s`.
///
/// Both are `None` until the estimator warms up, and render as a placeholder
/// rather than `0`, which would read as a stalled transfer.
fn rate_suffix(speed_bps: Option<f64>, eta_seconds: Option<f64>) -> String {
    format!(
        "{} · ETA {}",
        format_rate(speed_bps),
        format_duration(eta_seconds)
    )
}

impl Default for CliDownloadEventEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadEventEmitterPort for CliDownloadEventEmitter {
    fn emit(&self, event: DownloadEvent) {
        match event {
            DownloadEvent::DownloadStarted {
                id,
                shard_index,
                total_shards,
            } => {
                let label = match (shard_index, total_shards) {
                    (Some(idx), Some(total)) => {
                        format!("{id} [shard {}/{total}]", idx + 1)
                    }
                    _ => id.clone(),
                };

                self.start_bar(id, label);
            }

            DownloadEvent::DownloadProgress {
                id,
                downloaded,
                total,
                speed_bps,
                eta_seconds,
                ..
            } => {
                self.update_bar(
                    &id,
                    downloaded,
                    total,
                    &format!("{id} {}", rate_suffix(speed_bps, eta_seconds)),
                );
            }

            DownloadEvent::ShardProgress {
                id,
                shard_index,
                total_shards,
                aggregate_downloaded,
                aggregate_total,
                speed_bps,
                eta_seconds,
                ..
            } => {
                let label = format!(
                    "{id} [shard {}/{total_shards}] {}",
                    shard_index + 1,
                    rate_suffix(speed_bps, eta_seconds),
                );
                self.update_bar(&id, aggregate_downloaded, aggregate_total, &label);
            }

            DownloadEvent::DownloadCompleted { id, .. } => {
                if let Ok(mut bars) = self.bars.lock() {
                    if let Some(bar) = bars.remove(&id) {
                        bar.finish_with_message(format!("✓ {id}"));
                    }
                }
            }

            DownloadEvent::DownloadFailed { id, error } => {
                if let Ok(mut bars) = self.bars.lock() {
                    if let Some(bar) = bars.remove(&id) {
                        bar.abandon_with_message(format!("✗ {id}: {error}"));
                    }
                }
            }

            DownloadEvent::DownloadCancelled { id } => {
                if let Ok(mut bars) = self.bars.lock() {
                    if let Some(bar) = bars.remove(&id) {
                        bar.abandon_with_message(format!("✗ {id}: cancelled"));
                    }
                }
            }

            DownloadEvent::DownloadStatusChanged { id, status } => {
                // Surface non-terminal lifecycle transitions (Finalizing,
                // Registering) on the existing bar so the user sees that
                // work is still happening between "100% downloaded" and
                // the final completion event.
                if let Ok(bars) = self.bars.lock() {
                    if let Some(bar) = bars.get(&id) {
                        bar.set_message(format!("{id} — {}…", status.label()));
                    }
                }
            }

            DownloadEvent::DownloadNotice { id, message } => {
                // Same idea as DownloadStatusChanged, but for free-form setup
                // notes (e.g. building the first-run Python environment)
                // rather than a fixed lifecycle status. The next progress or
                // status event overwrites this naturally.
                if let Ok(bars) = self.bars.lock() {
                    if let Some(bar) = bars.get(&id) {
                        bar.set_message(format!("{id} — {message}"));
                    }
                }
            }

            // Queue-level events don't need bar updates in the CLI emitter.
            DownloadEvent::QueueSnapshot { .. } | DownloadEvent::QueueRunComplete { .. } => {}
        }
    }

    fn clone_box(&self) -> Box<dyn DownloadEventEmitterPort> {
        // CliDownloadEventEmitter is not Clone (Mutex), so we return a NoopDownloadEmitter
        // for the rare code path that needs a boxed clone. The real emitter is shared via Arc.
        Box::new(gglib_core::ports::NoopDownloadEmitter::new())
    }
}

/// Routes the CLI emitter through the unified `AppEventEmitter` pipeline.
///
/// This impl exists so the CLI bootstrap can pass an `Arc<dyn AppEventEmitter>`
/// to [`gglib_bootstrap::CoreBootstrap::build`] just like Axum and Tauri do.
/// The shared bootstrap wraps it in `AppEventBridge`, which converts
/// `DownloadEvent` → `AppEvent::Download { event }`. Here we unwrap that
/// variant and forward the inner `DownloadEvent` to the indicatif renderer.
///
/// Non-download `AppEvent` variants (server lifecycle, model lifecycle, MCP,
/// proxy) are deliberately ignored — the CLI has no UI surface for them.
impl AppEventEmitter for CliDownloadEventEmitter {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::Download { event } = event {
            <Self as DownloadEventEmitterPort>::emit(self, event);
        }
    }

    fn clone_box(&self) -> Box<dyn AppEventEmitter> {
        // See DownloadEventEmitterPort::clone_box above — the real emitter
        // is shared via Arc; this fallback is for the rare boxed-clone path.
        Box::new(gglib_core::ports::NoopEmitter::new())
    }
}
