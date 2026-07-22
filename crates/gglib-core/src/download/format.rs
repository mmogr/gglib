//! Display formatting for download rates and durations.
//!
//! Shared by every Rust renderer (the CLI progress bars in `gglib-download`)
//! and mirrored exactly by `formatRate` / `formatDuration` in
//! `src/utils/format.ts` so the CLI, the Tauri GUI and the web UI all render the
//! same number the same way.
//!
//! # Units
//!
//! **Rates are decimal** — `1 MB/s` is 1,000,000 bytes per second. This matches
//! Activity Monitor, `nettop`, `iftop` and every ISP, which is what users
//! compare a download speed against.
//!
//! **Sizes stay binary** (`MiB`, `GiB`) because that is the convention for
//! model files on disk, and are rendered by `indicatif`'s `HumanBytes`, which
//! already labels them correctly. Do not use this module for sizes.

/// Placeholder rendered when a value is not yet known.
///
/// An unknown rate is deliberately not `0`: zero is a real reading that means
/// "stalled", and conflating the two is what produced `ETA: 0s` on a download
/// that was progressing perfectly well.
pub const UNKNOWN: &str = "—";

const KB: f64 = 1_000.0;
const MB: f64 = 1_000_000.0;
const GB: f64 = 1_000_000_000.0;

/// Format a transfer rate in decimal units, e.g. `118.4 MB/s`.
///
/// Returns [`UNKNOWN`] for `None` and for values that are negative or not
/// finite.
#[must_use]
pub fn format_rate(bps: Option<f64>) -> String {
    let Some(bps) = bps.filter(|v| v.is_finite() && *v >= 0.0) else {
        return UNKNOWN.to_string();
    };

    if bps >= GB {
        format!("{:.2} GB/s", bps / GB)
    } else if bps >= MB {
        format!("{:.1} MB/s", bps / MB)
    } else if bps >= KB {
        format!("{:.0} kB/s", bps / KB)
    } else {
        format!("{bps:.0} B/s")
    }
}

/// Format a duration in seconds as `45s`, `3m 20s` or `1h 04m`.
///
/// Returns [`UNKNOWN`] for `None` and for values that are negative or not
/// finite. Sub-second values round up to `1s` so a live countdown never
/// displays `0s` while work is still in flight.
#[must_use]
pub fn format_duration(seconds: Option<f64>) -> String {
    let Some(seconds) = seconds.filter(|v| v.is_finite() && *v >= 0.0) else {
        return UNKNOWN.to_string();
    };

    // Saturate rather than wrap on absurd inputs (a near-zero rate can produce
    // an ETA of centuries before the average settles).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let total = seconds.ceil().min(359_999.0) as u64;

    let (hours, minutes, secs) = (total / 3600, (total % 3600) / 60, total % 60);

    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs:02}s")
    } else {
        format!("{}s", total.max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_rate_renders_a_placeholder() {
        assert_eq!(format_rate(None), UNKNOWN);
        assert_eq!(format_rate(Some(f64::NAN)), UNKNOWN);
        assert_eq!(format_rate(Some(f64::INFINITY)), UNKNOWN);
        assert_eq!(format_rate(Some(-1.0)), UNKNOWN);
    }

    #[test]
    fn rates_use_decimal_units() {
        assert_eq!(format_rate(Some(0.0)), "0 B/s");
        assert_eq!(format_rate(Some(999.0)), "999 B/s");
        assert_eq!(format_rate(Some(1_000.0)), "1 kB/s");
        assert_eq!(format_rate(Some(1_500_000.0)), "1.5 MB/s");
        assert_eq!(format_rate(Some(118_400_000.0)), "118.4 MB/s");
        assert_eq!(format_rate(Some(2_500_000_000.0)), "2.50 GB/s");
    }

    #[test]
    fn a_megabyte_per_second_is_a_million_bytes() {
        // The whole point of choosing decimal: this must agree with what a
        // system network monitor reports for the same transfer.
        assert_eq!(format_rate(Some(1_048_576.0)), "1.0 MB/s");
        assert_eq!(format_rate(Some(1_000_000.0)), "1.0 MB/s");
    }

    #[test]
    fn unknown_duration_renders_a_placeholder() {
        assert_eq!(format_duration(None), UNKNOWN);
        assert_eq!(format_duration(Some(f64::NAN)), UNKNOWN);
        assert_eq!(format_duration(Some(-5.0)), UNKNOWN);
    }

    #[test]
    fn durations_scale_by_magnitude() {
        assert_eq!(format_duration(Some(0.0)), "1s");
        assert_eq!(format_duration(Some(45.0)), "45s");
        assert_eq!(format_duration(Some(59.4)), "1m 00s");
        assert_eq!(format_duration(Some(200.0)), "3m 20s");
        assert_eq!(format_duration(Some(3_600.0)), "1h 00m");
        assert_eq!(format_duration(Some(3_845.0)), "1h 04m");
    }

    #[test]
    fn absurd_durations_saturate_instead_of_wrapping() {
        assert_eq!(format_duration(Some(1e18)), "99h 59m");
    }
}
