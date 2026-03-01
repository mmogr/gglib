//! Timing utility shared across crates that measure tool execution latency.

use std::time::Instant;

/// Convert `Instant::elapsed()` to whole milliseconds, clamping to `u64::MAX`.
///
/// Used wherever `wait_ms` / `duration_ms` fields are populated so that the
/// same `u64::try_from(…).unwrap_or(u64::MAX)` boilerplate is not repeated.
#[inline]
pub fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_ms_returns_small_value_immediately() {
        let start = Instant::now();
        let ms = elapsed_ms(start);
        assert!(
            ms < 1000,
            "elapsed_ms should be near zero for an immediate call"
        );
    }
}
