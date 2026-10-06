//! Tests of the download formatters.

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

/// 999,999 B/s is a megabyte a second to one decimal, and is not shown as
/// a thousand kilobytes.
#[test]
fn a_rate_takes_its_unit_after_rounding() {
    assert_eq!(format_rate(Some(999.5)), "1 kB/s");
    assert_eq!(format_rate(Some(999_999.0)), "1.0 MB/s");
    assert_eq!(format_rate(Some(999_999_999.0)), "1.00 GB/s");
    // Below the next unit after rounding, the unit stays.
    assert_eq!(format_rate(Some(999.4)), "999 B/s");
    assert_eq!(format_rate(Some(999_400.0)), "999 kB/s");
    assert_eq!(format_rate(Some(999_940_000.0)), "999.9 MB/s");
}

#[test]
fn sizes_are_binary_with_two_decimals() {
    assert_eq!(format_size(0), "0 B");
    assert_eq!(format_size(1023), "1023 B");
    assert_eq!(format_size(1024), "1.00 KiB");
    assert_eq!(format_size(1_000_000), "976.56 KiB");
    assert_eq!(format_size(1024 * 1024), "1.00 MiB");
    assert_eq!(format_size(29_980_000_000), "27.92 GiB");
    // The unit is taken after rounding here too.
    assert_eq!(format_size(1024 * 1024 - 1), "1.00 MiB");
    // 5.125 KiB exactly: a tie goes away from zero, as it does in the
    // TypeScript formatter. Printing the unrounded value would give 5.12.
    assert_eq!(format_size(5248), "5.13 KiB");
}

/// `format_vectors.json` is read by this test and by the TypeScript one, so
/// the two formatters give the same text for the same number.
#[test]
fn the_formatters_match_the_shared_vectors() {
    let vectors: serde_json::Value =
        serde_json::from_str(include_str!("format_vectors.json")).expect("the vectors parse");
    let rows = |key: &str| vectors[key].as_array().expect("an array").clone();
    let text = |row: &serde_json::Value| row["text"].as_str().expect("a text").to_string();

    for row in rows("rates") {
        assert_eq!(format_rate(row["bps"].as_f64()), text(&row), "{row}");
    }
    for row in rows("durations") {
        assert_eq!(
            format_duration(row["seconds"].as_f64()),
            text(&row),
            "{row}"
        );
    }
    for row in rows("sizes") {
        let bytes = row["bytes"].as_u64().expect("a byte count");
        assert_eq!(format_size(bytes), text(&row), "{row}");
    }
    assert!(rows("rates").len() >= 8 && rows("durations").len() >= 6 && rows("sizes").len() >= 6);
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
