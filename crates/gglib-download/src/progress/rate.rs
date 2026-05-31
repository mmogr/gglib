//! 8-second sliding window speed and ETA estimator.
//!
//! Provides [`SlidingWindowRate`] for accurate download speed estimation and
//! [`format_eta`] for consistent ETA formatting across all display paths.
//!
//! # Algorithm
//!
//! Speed = `(newest_bytes − oldest_bytes) / (newest_time − oldest_time)` over
//! the most recent [`SPEED_WINDOW`] seconds of samples. This matches the display
//! behaviour of `wget`, `aria2`, and `curl`, eliminating the tuning constants
//! required by EWA-based approaches.
//!
//! # Display parity
//!
//! All four download progress display paths — GUI SSE bridge
//! (`spawn_progress_bridge`), TTY CLI (`CliDownloadEventEmitter`),
//! fast-download TTY (`FancyProgress`), and non-TTY (`PlainProgress`) — share
//! this struct and [`format_eta`] so that the GUI and every CLI variant always
//! show the same speed and ETA values.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Duration of the sliding window used for speed estimation.
///
/// Wide enough to absorb short TCP-level bursts; narrow enough to track
/// genuine speed changes within a few seconds. Matches the typical display
/// window used by `wget` and `aria2`.
pub const SPEED_WINDOW: Duration = Duration::from_secs(8);

// ─── Internal sample type ─────────────────────────────────────────────────────

/// A single timestamped byte-count observation.
#[derive(Debug, Clone, Copy)]
struct Sample {
    time: Instant,
    bytes: u64,
}

// ─── SlidingWindowRate ────────────────────────────────────────────────────────

/// 8-second sliding window download speed estimator.
///
/// Records timestamped byte-count samples and computes bytes/sec from the
/// oldest and newest sample within [`SPEED_WINDOW`]. Falls back to an overall
/// average during the initial warm-up period (fewer than two distinct
/// timestamps in the window).
///
/// # Warm-up behaviour
///
/// Before the window contains two samples with distinct timestamps,
/// [`speed_bps`](SlidingWindowRate::speed_bps) uses the very first recorded
/// sample as the reference, giving a rough overall-average until the window
/// has enough data to produce a sliding estimate.
///
/// # Thread safety
///
/// Not `Send` or `Sync` on its own — callers hold it behind a `Mutex` or in
/// a single async task (the bridge spawns one per download).
pub struct SlidingWindowRate {
    /// Recent samples within the sliding window.
    samples: VecDeque<Sample>,
    /// The first sample ever recorded; used as the warm-up fallback reference.
    start: Option<Sample>,
}

impl SlidingWindowRate {
    /// Create a new, empty estimator.
    pub const fn new() -> Self {
        Self {
            samples: VecDeque::new(),
            start: None,
        }
    }

    /// Record a new byte-count observation at the given instant.
    ///
    /// Evicts samples older than [`SPEED_WINDOW`] before appending, so the
    /// deque always represents at most the last 8 seconds of history.
    pub fn record(&mut self, now: Instant, bytes: u64) {
        // Evict samples that have aged out of the window.
        while let Some(front) = self.samples.front() {
            if now.duration_since(front.time) > SPEED_WINDOW {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        let sample = Sample { time: now, bytes };

        if self.start.is_none() {
            self.start = Some(sample);
        }

        self.samples.push_back(sample);
    }

    /// Return the current speed estimate in bytes per second.
    ///
    /// Uses the oldest and newest in-window sample when they have distinct
    /// timestamps. Falls back to the overall average (bytes since the first
    /// [`record`](SlidingWindowRate::record) call) during the warm-up period.
    /// Returns `0.0` before any samples are recorded.
    pub fn speed_bps(&self) -> f64 {
        let (oldest, newest) = match (self.samples.front(), self.samples.back()) {
            (Some(o), Some(n)) => (*o, *n),
            _ => return 0.0,
        };

        if oldest.time == newest.time {
            // Only one effective timestamp in the window — fall back to
            // overall average from the very first sample.
            return self.start.map_or(0.0, |start| {
                let elapsed = newest.time.duration_since(start.time).as_secs_f64();
                if elapsed <= 0.0 {
                    return 0.0;
                }
                let bytes = newest.bytes.saturating_sub(start.bytes);
                #[allow(clippy::cast_precision_loss)]
                {
                    bytes as f64 / elapsed
                }
            });
        }

        let elapsed = newest.time.duration_since(oldest.time).as_secs_f64();
        if elapsed <= 0.0 {
            return 0.0;
        }
        let bytes = newest.bytes.saturating_sub(oldest.bytes);
        #[allow(clippy::cast_precision_loss)]
        {
            bytes as f64 / elapsed
        }
    }

    /// Reset the estimator, discarding all samples.
    ///
    /// Use when a download restarts or is replaced so stale history does not
    /// contaminate the new download's speed display.
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.samples.clear();
        self.start = None;
    }
}

// ─── format_eta ───────────────────────────────────────────────────────────────

/// Format a duration in seconds as a human-readable ETA string.
///
/// This is a standalone formatting utility shared by all download progress
/// display paths (GUI SSE bridge, TTY CLI, fast-download TTY, non-TTY) to
/// ensure consistent ETA presentation everywhere.
///
/// | Input | Output |
/// |---|---|
/// | `≤ 0.0`, `NaN`, or infinite | `"--:--"` |
/// | `< 3600.0` | `"M:SS"` |
/// | `≥ 3600.0` | `"H:MM:SS"` |
pub fn format_eta(secs: f64) -> String {
    if secs <= 0.0 || !secs.is_finite() {
        return "--:--".to_string();
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_eta ───────────────────────────────────────────────────────────

    #[test]
    fn format_eta_zero_returns_placeholder() {
        assert_eq!(format_eta(0.0), "--:--");
    }

    #[test]
    fn format_eta_negative_returns_placeholder() {
        assert_eq!(format_eta(-5.0), "--:--");
    }

    #[test]
    fn format_eta_infinity_returns_placeholder() {
        assert_eq!(format_eta(f64::INFINITY), "--:--");
    }

    #[test]
    fn format_eta_nan_returns_placeholder() {
        assert_eq!(format_eta(f64::NAN), "--:--");
    }

    #[test]
    fn format_eta_sub_minute() {
        assert_eq!(format_eta(45.0), "0:45");
    }

    #[test]
    fn format_eta_one_minute() {
        assert_eq!(format_eta(60.0), "1:00");
    }

    #[test]
    fn format_eta_sub_hour() {
        assert_eq!(format_eta(90.0), "1:30");
    }

    #[test]
    fn format_eta_exactly_one_hour() {
        assert_eq!(format_eta(3600.0), "1:00:00");
    }

    #[test]
    fn format_eta_super_hour() {
        assert_eq!(format_eta(3661.0), "1:01:01");
    }

    // ── SlidingWindowRate ────────────────────────────────────────────────────

    #[test]
    fn speed_zero_before_any_samples() {
        let rate = SlidingWindowRate::new();
        assert!(rate.speed_bps() == 0.0);
    }

    #[test]
    fn speed_zero_with_single_sample() {
        let mut rate = SlidingWindowRate::new();
        rate.record(Instant::now(), 0);
        // Only one distinct timestamp — falls back to overall avg; elapsed = 0
        assert!(rate.speed_bps() == 0.0);
    }

    #[test]
    fn two_samples_compute_correct_speed() {
        let mut rate = SlidingWindowRate::new();
        let t0 = Instant::now();
        rate.record(t0, 0);
        rate.record(t0 + Duration::from_secs(1), 1_000_000);
        let speed = rate.speed_bps();
        assert!((speed - 1_000_000.0).abs() < 1.0, "speed={speed}");
    }

    #[test]
    fn constant_rate_accuracy() {
        let mut rate = SlidingWindowRate::new();
        let t0 = Instant::now();
        // 1 MiB/s over 4 seconds (5 samples at 1s intervals)
        for i in 0u64..=4 {
            rate.record(t0 + Duration::from_secs(i), i * 1_048_576);
        }
        let speed = rate.speed_bps();
        assert!((speed - 1_048_576.0).abs() < 1_000.0, "speed={speed}");
    }

    #[test]
    fn window_evicts_old_samples() {
        let mut rate = SlidingWindowRate::new();
        let t0 = Instant::now();
        // Add a sample well outside the 8-second window
        rate.record(t0, 0);
        // Jump 20 seconds forward and add two fresh samples at 1 MiB/s
        let t1 = t0 + Duration::from_secs(20);
        rate.record(t1, 10_000_000);
        rate.record(t1 + Duration::from_secs(1), 10_000_000 + 1_048_576);
        // The old sample should have been evicted; speed reflects only recent data
        let speed = rate.speed_bps();
        assert!((speed - 1_048_576.0).abs() < 1_000.0, "speed={speed}");
    }

    #[test]
    fn reset_clears_all_state() {
        let mut rate = SlidingWindowRate::new();
        let t0 = Instant::now();
        rate.record(t0, 0);
        rate.record(t0 + Duration::from_secs(1), 1_000_000);
        assert!(rate.speed_bps() > 0.0);
        rate.reset();
        assert!(rate.speed_bps() == 0.0);
    }
}
