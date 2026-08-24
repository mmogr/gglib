//! Progress throttling.
//!
//! Rate-limits progress *emission* so a fast transfer cannot flood a channel
//! or a redraw loop with events nobody can read.
//!
//! Deliberately beside [`RateEstimator`](super::rate::RateEstimator): the two
//! are always used together and the pairing is load-bearing. Feed the
//! estimator every tick and throttle only what you send — throttling the
//! measurement is what makes a reported rate drift.

use std::time::{Duration, Instant};

/// Rate-limiter for progress updates.
///
/// Ensures progress events are not emitted more frequently than the
/// configured interval.
pub struct ProgressThrottle {
    last_emit: Option<Instant>,
    min_interval: Duration,
}

impl ProgressThrottle {
    /// Create a new throttle with the specified minimum interval.
    pub const fn new(min_interval: Duration) -> Self {
        Self {
            last_emit: None,
            min_interval,
        }
    }

    /// Create a throttle with a default interval of 100ms.
    pub const fn default_interval() -> Self {
        Self::new(Duration::from_millis(100))
    }

    /// Check if enough time has passed to emit another progress update.
    pub fn should_emit(&mut self) -> bool {
        let now = Instant::now();
        match self.last_emit {
            Some(last) if now.duration_since(last) < self.min_interval => false,
            _ => {
                self.last_emit = Some(now);
                true
            }
        }
    }

    /// Force the next check to return true.
    pub const fn reset(&mut self) {
        self.last_emit = None;
    }
}

impl Default for ProgressThrottle {
    fn default() -> Self {
        Self::default_interval()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_throttle_first_emit() {
        let mut throttle = ProgressThrottle::new(Duration::from_millis(100));
        assert!(throttle.should_emit()); // First call should always emit
    }

    #[test]
    fn test_throttle_respects_interval() {
        let mut throttle = ProgressThrottle::new(Duration::from_millis(50));
        assert!(throttle.should_emit());
        assert!(!throttle.should_emit()); // Too soon

        std::thread::sleep(Duration::from_millis(60));
        assert!(throttle.should_emit()); // Enough time passed
    }

    #[test]
    fn test_throttle_reset() {
        let mut throttle = ProgressThrottle::new(Duration::from_millis(100));
        throttle.should_emit();
        assert!(!throttle.should_emit());

        throttle.reset();
        assert!(throttle.should_emit()); // Reset allows immediate emit
    }
}
