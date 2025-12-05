//! Progress throttling utilities for download operations.
//!
//! This module provides throttling utilities to limit the frequency of progress
//! updates during downloads, reducing UI update overhead while maintaining
//! responsive feedback.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct ProgressThrottle {
    state: Arc<Mutex<ThrottleState>>,
    min_interval: Duration,
    min_step_bytes: u64,
}

struct ThrottleState {
    last_emit: Instant,
    last_bytes: u64,
    emitted: bool,
    /// Exponentially weighted average speed in bytes/sec
    ewa_speed: f64,
}

/// Smoothing factor for EWA speed calculation.
/// 0.02 = 2% weight to new sample, 98% to historical average.
/// This provides ~10 second response time for very stable speed display.
const EWA_SMOOTHING: f64 = 0.02;

impl ProgressThrottle {
    pub fn new(min_interval: Duration, min_step_bytes: u64) -> Self {
        Self {
            state: Arc::new(Mutex::new(ThrottleState {
                last_emit: Instant::now(),
                last_bytes: 0,
                emitted: false,
                ewa_speed: 0.0,
            })),
            min_interval,
            min_step_bytes,
        }
    }

    /// Tuned defaults for interactive UI progress bars (CLI+GUI).
    pub fn responsive_ui() -> Self {
        Self::new(Duration::from_millis(150), 512 * 1_024)
    }

    /// Check if we should emit a progress event, and if so, update EWA speed.
    /// Returns Some(ewa_speed) if should emit, None otherwise.
    pub fn should_emit_with_speed(&self, downloaded: u64, total: u64) -> Option<f64> {
        let mut state = self.state.lock().expect("progress throttle lock poisoned");
        let now = Instant::now();
        let elapsed = now.duration_since(state.last_emit);
        let advanced = downloaded.saturating_sub(state.last_bytes);
        let force_emit = !state.emitted || (total > 0 && downloaded >= total);

        if !(force_emit || elapsed >= self.min_interval || advanced >= self.min_step_bytes) {
            return None;
        }

        // Calculate instantaneous speed for this interval
        let elapsed_secs = elapsed.as_secs_f64();
        let instant_speed = if elapsed_secs > 0.0 {
            advanced as f64 / elapsed_secs
        } else {
            0.0
        };

        // Update EWA speed: new_ewa = smoothing * instant + (1 - smoothing) * old_ewa
        // On first emit, just use the instant speed
        if state.emitted {
            state.ewa_speed =
                EWA_SMOOTHING * instant_speed + (1.0 - EWA_SMOOTHING) * state.ewa_speed;
        } else {
            state.ewa_speed = instant_speed;
        }

        state.last_emit = now;
        state.last_bytes = downloaded;
        state.emitted = true;
        Some(state.ewa_speed)
    }

    /// Legacy method for backwards compatibility - just returns bool.
    pub fn should_emit(&self, downloaded: u64, total: u64) -> bool {
        self.should_emit_with_speed(downloaded, total).is_some()
    }

    /// Get the current EWA speed without checking throttle.
    pub fn current_speed(&self) -> f64 {
        self.state
            .lock()
            .expect("progress throttle lock poisoned")
            .ewa_speed
    }
}
