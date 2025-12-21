//! Table formatting utilities for CLI output.

/// Truncates a string to a maximum length, adding "..." if needed.
///
/// # Examples
///
/// ```rust
/// use gglib_cli::presentation::truncate_string;
///
/// assert_eq!(truncate_string("Hello", 10), "Hello");
/// assert_eq!(truncate_string("Hello World", 8), "Hello...");
/// ```
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
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
