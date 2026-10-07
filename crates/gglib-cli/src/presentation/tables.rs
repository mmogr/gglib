//! Table formatting utilities for CLI output.

use chrono::{NaiveDateTime, Utc};

/// Format a `SQLite` datetime string as a human-readable relative time.
///
/// Returns strings like "just now", "5 min ago", "3 hours ago", "2 days ago",
/// or the original date if more than 30 days old.
///
/// Falls back to the raw string on parse failure.
pub(crate) fn format_relative_time(datetime_str: &str) -> String {
    let Ok(dt) = NaiveDateTime::parse_from_str(datetime_str, "%Y-%m-%d %H:%M:%S") else {
        return datetime_str.to_string();
    };
    let now = Utc::now().naive_utc();
    let delta = now.signed_duration_since(dt);
    let secs = delta.num_seconds();

    if secs < 0 {
        return datetime_str.to_string();
    }

    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let m = secs / 60;
            format!("{m} min ago")
        }
        3600..=86399 => {
            let h = secs / 3600;
            if h == 1 {
                "1 hour ago".to_string()
            } else {
                format!("{h} hours ago")
            }
        }
        86400..=2_591_999 => {
            let d = secs / 86400;
            if d == 1 {
                "yesterday".to_string()
            } else {
                format!("{d} days ago")
            }
        }
        _ => dt.format("%Y-%m-%d").to_string(),
    }
}

/// Truncates a string to at most `max_len` characters, appending `\u{2026}` (…)
/// if the string is longer.
///
/// Counts Unicode characters, not bytes, so multi-byte UTF-8 sequences are
/// handled correctly. The output is always at most `max_len` characters wide.
///
/// # Examples
///
/// Illustrative rather than executable: this function is crate-internal, and a
/// doctest compiles as its own crate, so no import path can name it. Fenced as
/// `text` rather than `ignore` so rustdoc renders it as prose instead of an
/// untested example. The two cases below are asserted for real in this
/// module's tests.
///
/// ```text
/// truncate_string("Hello", 10)       == "Hello"
/// truncate_string("Hello World", 8)  == "Hello W…"
/// ```
pub(crate) fn truncate_string(s: &str, max_len: usize) -> String {
    truncate_with(s, max_len, "\u{2026}")
}

/// [`truncate_string`] with the mark of the caller's choosing: `s` when it has
/// at most `max_len` characters, and otherwise its start with `marker` after
/// it, `max_len` characters in all.
pub(crate) fn truncate_with(s: &str, max_len: usize, marker: &str) -> String {
    if s.chars().count() <= max_len {
        return s.to_string();
    }
    let kept = max_len.saturating_sub(marker.chars().count());
    format!("{}{marker}", first_chars(s, kept))
}

/// The first `max` characters of `s`, or all of it when it has no more.
///
/// The cut is made by characters: a byte offset can land inside one, and
/// slicing there panics.
pub(crate) fn first_chars(s: &str, max: usize) -> &str {
    s.char_indices().nth(max).map_or(s, |(end, _)| &s[..end])
}

/// The first 8 characters of a commit SHA or a file hash, or all of it when
/// it is shorter. `HuggingFace` returns 40 and 64, but a truncated or empty
/// value must not panic a command whose whole job is repairing a model.
pub(crate) fn short_sha(sha: &str) -> &str {
    first_chars(sha, 8)
}

/// Format large numbers with K/M suffixes.
#[allow(
    clippy::cast_precision_loss,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub(crate) fn format_number(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Print a horizontal separator line.
pub(crate) fn print_separator(width: usize) {
    println!("{}", "-".repeat(width));
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate_string("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate_string("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string_gets_ellipsis() {
        // max_len=5: 4 chars of content + ellipsis = 5 chars total
        let result = truncate_string("hello world", 5);
        assert_eq!(result, "hell\u{2026}");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate_string("", 10), "");
    }

    /// One-byte letters up to byte `at`, one `wide` character there, and
    /// enough after it that the whole is longer than any limit tried.
    fn wide_at(at: usize, wide: char) -> String {
        format!("{}{wide}{}", "a".repeat(at), "b".repeat(120))
    }

    /// The cuts `model search` and `model browse` make of a description, and
    /// the tables' own: a character of three or four bytes at every byte
    /// offset around the cut comes out whole or not at all.
    #[test]
    fn a_cut_never_lands_inside_a_character() {
        for wide in ['\u{1f600}', '\u{8a9e}'] {
            for (limit, marker) in [(80, "..."), (100, "..."), (24, "\u{2026}")] {
                for at in limit - 8..=limit + 2 {
                    let text = wide_at(at, wide);

                    let cut = truncate_with(&text, limit, marker);

                    let kept = limit - marker.chars().count();
                    let start: String = text.chars().take(kept).collect();
                    assert_eq!(cut, format!("{start}{marker}"), "{wide} at byte {at}");
                    assert_eq!(cut.chars().count(), limit, "{wide} at byte {at}");
                }
            }
        }
    }

    /// Text of one-byte characters is cut as it always was: 77 of them and
    /// three dots at 80, and nothing at all when it fits.
    #[test]
    fn a_description_of_plain_letters_keeps_its_old_cut() {
        let fits = "x".repeat(80);
        let over = "x".repeat(81);

        assert_eq!(truncate_with(&fits, 80, "..."), fits);
        assert_eq!(
            truncate_with(&over, 80, "..."),
            format!("{}...", "x".repeat(77))
        );
        assert_eq!(
            truncate_with(&"x".repeat(101), 100, "..."),
            format!("{}...", "x".repeat(97))
        );
    }

    /// Characters are counted, not bytes: text that fits is left whole however
    /// many bytes it takes.
    #[test]
    fn text_that_fits_in_characters_is_left_whole() {
        let cjk = "\u{8a9e}".repeat(80);
        assert_eq!(truncate_with(&cjk, 80, "..."), cjk);
    }

    #[test]
    fn the_first_characters_of_text_are_whole_characters() {
        for wide in ['\u{1f600}', '\u{8a9e}'] {
            for at in 0..=13 {
                let text = wide_at(at, wide);
                for max in [8, 12] {
                    let start: String = text.chars().take(max).collect();
                    assert_eq!(first_chars(&text, max), start, "{wide} at byte {at}");
                }
            }
        }
        assert_eq!(first_chars("abc", 8), "abc");
        assert_eq!(first_chars("", 8), "");
        assert_eq!(first_chars("abc", 0), "");
    }

    #[test]
    fn a_short_sha_is_eight_characters_or_all_there_is() {
        assert_eq!(
            short_sha("0123456789abcdef0123456789abcdef01234567"),
            "01234567"
        );
        assert_eq!(short_sha("abc"), "abc");
        assert_eq!(short_sha(""), "");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(1_500), "1.5K");
        assert_eq!(format_number(1_500_000), "1.5M");
    }

    #[test]
    fn relative_time_just_now() {
        let now = Utc::now()
            .naive_utc()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(format_relative_time(&now), "just now");
    }

    #[test]
    fn relative_time_minutes_ago() {
        let ts = (Utc::now().naive_utc() - chrono::Duration::minutes(5))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(format_relative_time(&ts), "5 min ago");
    }

    #[test]
    fn relative_time_hours_ago() {
        let ts = (Utc::now().naive_utc() - chrono::Duration::hours(3))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(format_relative_time(&ts), "3 hours ago");
    }

    #[test]
    fn relative_time_yesterday() {
        let ts = (Utc::now().naive_utc() - chrono::Duration::days(1))
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        assert_eq!(format_relative_time(&ts), "yesterday");
    }

    #[test]
    fn relative_time_old_shows_date() {
        assert_eq!(format_relative_time("2020-01-15 10:30:00"), "2020-01-15");
    }

    #[test]
    fn relative_time_bad_parse_returns_raw() {
        assert_eq!(format_relative_time("not-a-date"), "not-a-date");
    }
}
