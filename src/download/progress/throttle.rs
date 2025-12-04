//! Progress throttling with exponentially weighted average (EWA) speed calculation.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Smoothing factor for EWA speed calculation.
/// 0.02 = 2% weight to new sample, 98% to historical average.
/// This provides ~10 second response time for very stable speed display.
const EWA_SMOOTHING: f64 = 0.02;

/// Internal state for progress throttling.
struct ThrottleState {
    last_emit: Instant,
    last_bytes: u64,
    emitted: bool,
    /// Exponentially weighted average speed in bytes/sec.
    ewa_speed: f64,
}

/// Progress throttle with EWA speed calculation.
///
/// This type is `Clone` and can be shared across threads.
/// It ensures progress events are not emitted too frequently while
/// maintaining a smooth, stable speed calculation.
///
/// # Example
///
/// ```rust
/// use gglib::download::ProgressThrottle;
///
/// let throttle = ProgressThrottle::responsive_ui();
///
/// // During download loop:
/// if let Some(speed) = throttle.should_emit_with_speed(downloaded, total) {
///     // Emit progress event with `speed` in bytes/sec
/// }
/// ```
#[derive(Clone)]
pub struct ProgressThrottle {
    state: Arc<Mutex<ThrottleState>>,
    min_interval: Duration,
    min_step_bytes: u64,
}

impl ProgressThrottle {
    /// Create a new progress throttle.
    ///
    /// # Arguments
    ///
    /// * `min_interval` - Minimum time between progress emissions
    /// * `min_step_bytes` - Minimum bytes advanced before emitting
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
    ///
    /// - 150ms minimum interval (smooth animation without flicker)
    /// - 512KB minimum step (avoid micro-updates)
    pub fn responsive_ui() -> Self {
        Self::new(Duration::from_millis(150), 512 * 1_024)
    }

    /// Check if we should emit a progress event, and if so, return EWA speed.
    ///
    /// Returns `Some(ewa_speed)` if should emit, `None` otherwise.
    /// The returned speed is in bytes per second, smoothed using an
    /// exponentially weighted average.
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

    /// Reset the throttle state (for a new download).
    pub fn reset(&self) {
        let mut state = self.state.lock().expect("progress throttle lock poisoned");
        state.last_emit = Instant::now();
        state.last_bytes = 0;
        state.emitted = false;
        state.ewa_speed = 0.0;
    }
}

impl Default for ProgressThrottle {
    fn default() -> Self {
        Self::responsive_ui()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_first_emit_always_succeeds() {
        let throttle = ProgressThrottle::new(Duration::from_secs(10), 1_000_000);
        // First emit should always succeed regardless of interval/step
        assert!(throttle.should_emit_with_speed(100, 1000).is_some());
    }

    #[test]
    fn test_throttle_respects_interval() {
        // Use high step threshold so only interval matters
        let throttle = ProgressThrottle::new(Duration::from_millis(50), 1_000_000);

        // First emit
        throttle.should_emit_with_speed(100, 1000);

        // Immediate second emit should be throttled (neither interval nor step exceeded)
        assert!(throttle.should_emit_with_speed(200, 1000).is_none());

        // After waiting, should emit
        thread::sleep(Duration::from_millis(60));
        assert!(throttle.should_emit_with_speed(300, 1000).is_some());
    }

    #[test]
    fn test_throttle_respects_step() {
        let throttle = ProgressThrottle::new(Duration::from_secs(10), 1000);

        // First emit
        throttle.should_emit_with_speed(0, 10000);

        // Small advance should be throttled
        assert!(throttle.should_emit_with_speed(500, 10000).is_none());

        // Large advance should emit
        assert!(throttle.should_emit_with_speed(2000, 10000).is_some());
    }

    #[test]
    fn test_force_emit_on_completion() {
        let throttle = ProgressThrottle::new(Duration::from_secs(10), 1_000_000);

        // First emit
        throttle.should_emit_with_speed(0, 1000);

        // Completion should force emit
        assert!(throttle.should_emit_with_speed(1000, 1000).is_some());
    }

    #[test]
    fn test_ewa_speed_smoothing() {
        let throttle = ProgressThrottle::new(Duration::from_millis(1), 0);

        // First emit establishes baseline
        throttle.should_emit_with_speed(0, 10000);
        thread::sleep(Duration::from_millis(10));

        // Second emit with 1000 bytes in ~10ms = ~100KB/s
        throttle.should_emit_with_speed(1000, 10000);
        let speed1 = throttle.current_speed();

        // Speed should be positive
        assert!(speed1 > 0.0);
    }

    #[test]
    fn test_reset() {
        let throttle = ProgressThrottle::new(Duration::from_millis(100), 1000);

        // Emit and build up some state
        throttle.should_emit_with_speed(1000, 10000);

        // Reset
        throttle.reset();

        // First emit should succeed again
        assert!(throttle.should_emit_with_speed(0, 10000).is_some());
        assert_eq!(throttle.current_speed(), 0.0);
    }

    #[test]
    fn test_clone_shares_state() {
        // Use high step threshold so only interval matters
        let throttle1 = ProgressThrottle::new(Duration::from_secs(10), 1_000_000);
        let throttle2 = throttle1.clone();

        // Emit on throttle1
        throttle1.should_emit_with_speed(100, 1000);

        // Should be throttled on throttle2 as well (shared state, neither interval nor step exceeded)
        assert!(throttle2.should_emit_with_speed(200, 1000).is_none());
    }
}
