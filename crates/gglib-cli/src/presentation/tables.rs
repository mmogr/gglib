//! Table formatting utilities for CLI output.

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
