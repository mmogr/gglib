//! Display formatting for download rates, durations and sizes.
//!
//! Every download string a user reads is made from these, by
//! [`DownloadRowText`](super::DownloadRowText). `formatRate`,
//! `formatDuration` and `formatSize` in `src/utils/format.ts` are the same
//! functions in TypeScript, and `format_vectors.json` beside this file is
//! read by the tests of both, so the two cannot drift.
//!
//! # Units
//!
//! **Rates are decimal** — `1 MB/s` is 1,000,000 bytes per second. This matches
//! Activity Monitor, `nettop`, `iftop` and every ISP, which is what users
//! compare a download speed against.
//!
//! **Sizes are binary** (`MiB`, `GiB`) because that is the convention for
//! model files on disk.
//!
//! # Rounding
//!
//! A value is rounded first and its unit chosen after, so 999,999 B/s reads
//! `1.0 MB/s` and never `1000 kB/s`. Rounding is half away from zero on the
//! scaled value, `round(x * 10^d) / 10^d`, which is the one rule Rust and
//! JavaScript both give the same answer for.

/// Placeholder rendered when a value is not yet known.
///
/// An unknown rate is deliberately not `0`: zero is a real reading that means
/// "stalled", and conflating the two is what produced `ETA: 0s` on a download
/// that was progressing perfectly well.
pub(crate) const UNKNOWN: &str = "—";

const KB: f64 = 1_000.0;
const MB: f64 = 1_000_000.0;
const GB: f64 = 1_000_000_000.0;

/// `value` rounded to `decimals` places, half away from zero.
fn rounded(value: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (value * scale).round() / scale
}

/// Format a transfer rate in decimal units, e.g. `118.4 MB/s`.
///
/// Returns [`UNKNOWN`] for `None` and for values that are negative or not
/// finite.
#[must_use]
pub fn format_rate(bps: Option<f64>) -> String {
    let Some(bps) = bps.filter(|v| v.is_finite() && *v >= 0.0) else {
        return UNKNOWN.to_string();
    };

    let bytes = rounded(bps, 0);
    if bytes < KB {
        return format!("{bytes:.0} B/s");
    }
    let kilo = rounded(bps / KB, 0);
    if kilo < 1_000.0 {
        return format!("{kilo:.0} kB/s");
    }
    let mega = rounded(bps / MB, 1);
    if mega < 1_000.0 {
        return format!("{mega:.1} MB/s");
    }
    format!("{:.2} GB/s", rounded(bps / GB, 2))
}

/// Format a size in binary units with two decimals, e.g. `27.92 GiB`.
///
/// A size below 1024 bytes is a whole number of bytes.
#[must_use]
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }

    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64;
    let mut shown = value;
    let mut unit = UNITS[0];
    for next in UNITS {
        value /= 1024.0;
        shown = rounded(value, 2);
        unit = next;
        if shown < 1024.0 {
            break;
        }
    }
    format!("{shown:.2} {unit}")
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
#[path = "format_tests.rs"]
mod tests;
