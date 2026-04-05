//! Table formatting utilities for CLI output.

use chrono::{NaiveDateTime, Utc};

/// Format a SQLite datetime string as a human-readable relative time.
///
/// Returns strings like "just now", "5 min ago", "3 hours ago", "2 days ago",
/// or the original date if more than 30 days old.
///
/// Falls back to the raw string on parse failure.
pub fn format_relative_time(datetime_str: &str) -> String {
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
/// ```rust
/// use gglib_cli::presentation::truncate_string;
///
/// assert_eq!(truncate_string("Hello", 10), "Hello");
/// assert_eq!(truncate_string("Hello World", 8), "Hello W\u{2026}");
/// ```
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        // String fits — return as-is (no allocation needed beyond this clone).
        s.to_string()
    } else {
        // String exceeds max_len — take max_len-1 chars and append the ellipsis
        // so that the output is exactly max_len characters wide.
        let prefix: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{prefix}…")
    }
}

/// Print a horizontal separator line.
pub fn print_separator(width: usize) {
    println!("{}", "-".repeat(width));
}

/// Format an optional value for table display, returning a default if None.
pub fn format_optional<T: std::fmt::Display>(value: &Option<T>, default: &str) -> String {
    match value {
        Some(v) => v.to_string(),
        None => default.to_string(),
    }
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

    #[test]
    fn relative_time_just_now() {
        let now = Utc::now().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
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
