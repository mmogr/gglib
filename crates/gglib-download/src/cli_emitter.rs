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

use indicatif::{HumanBytes, MultiProgress, ProgressBar, ProgressStyle};

use gglib_core::download::DownloadEvent;
use gglib_core::ports::DownloadEventEmitterPort;

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
const BAR_TEMPLATE: &str = "{spinner:.cyan} {wide_msg} [{bar:30.cyan/blue}] {bytes}/{total_bytes} @ {bytes_per_sec} eta {eta}";
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
}

impl CliDownloadEventEmitter {
    /// Create a new emitter backed by a fresh [`MultiProgress`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            multi_progress: Arc::new(MultiProgress::new()),
            bars: Mutex::new(HashMap::new()),
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

                // Always create the bar with the unified BAR_TEMPLATE — see
                // module-level comment on `BAR_TEMPLATE` for why we never
                // switch styles mid-stream. Length 0 is a sentinel meaning
                // "total not yet known"; the bar widget renders empty until
                // a Progress/ShardProgress event supplies a real length.
                let style = ProgressStyle::with_template(BAR_TEMPLATE)
                    .unwrap_or_else(|_| ProgressStyle::default_bar())
                    .progress_chars("█▓░")
                    .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]);

                let bar = self.multi_progress.add(ProgressBar::new(0));
                bar.set_style(style);
                bar.enable_steady_tick(TICK_INTERVAL);
                bar.set_message(label);

                if let Ok(mut bars) = self.bars.lock() {
                    bars.insert(id, bar);
                }
            }

            DownloadEvent::DownloadProgress {
                id,
                downloaded,
                total,
                speed_bps: _,
                eta_seconds: _,
                percentage: _,
            } => {
                if let Ok(bars) = self.bars.lock() {
                    if let Some(bar) = bars.get(&id) {
                        // First time we see a real total, set length. No
                        // set_style — the unified template is already in
                        // place from DownloadStarted.
                        if total > 0 && bar.length().unwrap_or(0) == 0 {
                            bar.set_length(total);
                        }
                        bar.set_position(downloaded);
                        bar.set_message(format!("{id} {}", HumanBytes(downloaded)));
                    }
                }
            }

            DownloadEvent::ShardProgress {
                id,
                shard_index,
                total_shards,
                aggregate_downloaded,
                aggregate_total,
                ..
            } => {
                if let Ok(bars) = self.bars.lock() {
                    if let Some(bar) = bars.get(&id) {
                        // Update length on first real total or whenever the
                        // total changes (e.g. final shard size resolved).
                        // No set_style — unified template stays in place.
                        if aggregate_total > 0 && bar.length().unwrap_or(0) != aggregate_total {
                            bar.set_length(aggregate_total);
                        }
                        bar.set_position(aggregate_downloaded);
                        bar.set_message(format!(
                            "{id} [shard {}/{total_shards}] {}",
                            shard_index + 1,
                            HumanBytes(aggregate_downloaded),
                        ));
                    }
                }
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
