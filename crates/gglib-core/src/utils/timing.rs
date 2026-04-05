//! Timing utilities shared across crates.

use std::time::Instant;

/// Convert `Instant::elapsed()` to whole milliseconds, clamping to `u64::MAX`.
///
/// Used wherever `wait_ms` / `duration_ms` fields are populated so that the
/// same `u64::try_from(…).unwrap_or(u64::MAX)` boilerplate is not repeated.
#[inline]
pub fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Format a millisecond duration into a compact human-readable string.
///
/// ```
/// use gglib_core::utils::timing::format_duration_human;
///
/// assert_eq!(format_duration_human(0), "0ms");
/// assert_eq!(format_duration_human(125), "125ms");
/// assert_eq!(format_duration_human(1500), "1.5s");
/// assert_eq!(format_duration_human(60_000), "1m 0s");
/// assert_eq!(format_duration_human(135_000), "2m 15s");
/// ```
pub fn format_duration_human(ms: u64) -> String {
    if ms < 1_000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
#[allow(clippy::cast_precision_loss)] // ms values are ≤60k here; no precision issue
        {
            format!("{:.1}s", ms as f64 / 1_000.0)
        }
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1_000;
        format!("{mins}m {secs}s")
    }
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

    #[test]
    fn duration_human_milliseconds() {
        assert_eq!(format_duration_human(0), "0ms");
        assert_eq!(format_duration_human(125), "125ms");
        assert_eq!(format_duration_human(999), "999ms");
    }

    #[test]
    fn duration_human_seconds() {
        assert_eq!(format_duration_human(1_000), "1.0s");
        assert_eq!(format_duration_human(1_500), "1.5s");
        assert_eq!(format_duration_human(59_999), "60.0s");
    }

    #[test]
    fn duration_human_minutes() {
        assert_eq!(format_duration_human(60_000), "1m 0s");
        assert_eq!(format_duration_human(135_000), "2m 15s");
    }
}
