//! Shared tool display formatting — single source of truth for all surfaces.
//!
//! These pure functions convert raw tool names and arguments into
//! human-readable display strings.  The agentic loop populates
//! [`AgentEvent`] payloads with pre-formatted fields computed here,
//! so CLI, `WebUI` (Axum SSE), and GUI (Tauri) all render identical labels
//! without duplicating formatting logic.
//!
//! [`AgentEvent`]: super::events::AgentEvent

/// Strip the routing prefix from a qualified tool name.
///
/// Tool names carry a `"builtin:"` or `"{server_id}:"` prefix for
/// O(1) dispatch routing in `CombinedToolExecutor`.  This function
/// removes that prefix for display purposes only.
///
/// ```
/// use gglib_core::domain::agent::tool_display::strip_tool_prefix;
///
/// assert_eq!(strip_tool_prefix("builtin:read_file"), "read_file");
/// assert_eq!(strip_tool_prefix("3:some_tool"), "some_tool");
/// assert_eq!(strip_tool_prefix("plain_name"), "plain_name");
/// ```
pub fn strip_tool_prefix(name: &str) -> &str {
    name.find(':').map_or(name, |pos| &name[pos + 1..])
}

/// Convert a raw tool name into a human-readable "Title Case" label.
///
/// Splits on hyphens, underscores, dots, and whitespace, then title-cases
/// each word.  This replaces the frontend `formatToolDisplayName` function
/// and is the single Rust source of truth used by all surfaces.
///
/// ```
/// use gglib_core::domain::agent::tool_display::format_tool_display_name;
///
/// assert_eq!(format_tool_display_name("read_file"), "Read File");
/// assert_eq!(format_tool_display_name("get-weather"), "Get Weather");
/// assert_eq!(format_tool_display_name("file.read"), "File Read");
/// assert_eq!(format_tool_display_name("get_current_time"), "Get Current Time");
/// assert_eq!(format_tool_display_name("Already Good"), "Already Good");
/// ```
pub fn format_tool_display_name(raw: &str) -> String {
    raw.split(|c: char| c == '-' || c == '_' || c == '.' || c.is_whitespace())
        .filter(|w| !w.is_empty())
        .map(title_case_word)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract a one-line argument summary from a tool call's `arguments` JSON.
///
/// Known builtins get tool-specific summaries (e.g. `read_file → path`).
/// Unknown tools show the first string-valued key, truncated to 60 chars.
/// Returns `None` when no meaningful summary can be extracted.
///
/// ```
/// use gglib_core::domain::agent::tool_display::format_tool_args_summary;
/// use serde_json::json;
///
/// let args = json!({"path": "/src/main.rs", "line_range": [1, 50]});
/// assert_eq!(
///     format_tool_args_summary("read_file", &args),
///     Some("/src/main.rs".to_string()),
/// );
///
/// let args = json!({"pattern": "TODO", "path": "/src"});
/// assert_eq!(
///     format_tool_args_summary("grep_search", &args),
///     Some("\"TODO\" in /src".to_string()),
/// );
/// ```
pub fn format_tool_args_summary(bare_name: &str, arguments: &serde_json::Value) -> Option<String> {
    let obj = arguments.as_object()?;

    match bare_name {
        "read_file" => obj
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| truncate(s, 60).to_string()),

        "list_directory" => obj
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| truncate(s, 60).to_string()),

        "grep_search" => {
            let pattern = obj.get("pattern").and_then(|v| v.as_str())?;
            let path = obj.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            Some(format!(
                "\"{}\" in {}",
                truncate(pattern, 30),
                truncate(path, 30)
            ))
        }

        "get_current_time" => obj
            .get("timezone")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string),

        // Generic fallback: show the first string-valued argument.
        _ => obj
            .values()
            .find_map(|v| v.as_str())
            .map(|s| truncate(s, 60).to_string()),
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Title-case a single word (first char uppercase, rest lowercase).
fn title_case_word(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |c| {
        let upper: String = c.to_uppercase().collect();
        upper + chars.as_str()
    })
}

/// Truncate a string to `max_len` characters, appending `…` if truncated.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        // Find a valid char boundary at or before max_len.
        let end = s.floor_char_boundary(max_len);
        &s[..end]
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── strip_tool_prefix ────────────────────────────────────────────

    #[test]
    fn strip_builtin_prefix() {
        assert_eq!(strip_tool_prefix("builtin:read_file"), "read_file");
    }

    #[test]
    fn strip_numeric_server_prefix() {
        assert_eq!(strip_tool_prefix("3:some_tool"), "some_tool");
    }

    #[test]
    fn strip_no_prefix() {
        assert_eq!(strip_tool_prefix("plain_name"), "plain_name");
    }

    // ── format_tool_display_name ─────────────────────────────────────

    #[test]
    fn display_name_underscore() {
        assert_eq!(format_tool_display_name("read_file"), "Read File");
    }

    #[test]
    fn display_name_hyphen() {
        assert_eq!(format_tool_display_name("get-weather"), "Get Weather");
    }

    #[test]
    fn display_name_dot() {
        assert_eq!(format_tool_display_name("file.read"), "File Read");
    }

    #[test]
    fn display_name_mixed_separators() {
        assert_eq!(
            format_tool_display_name("my-tool_name.here"),
            "My Tool Name Here"
        );
    }

    #[test]
    fn display_name_consecutive_separators() {
        assert_eq!(format_tool_display_name("a..b"), "A B");
        assert_eq!(format_tool_display_name("a--b"), "A B");
    }

    #[test]
    fn display_name_single_word() {
        assert_eq!(format_tool_display_name("weather"), "Weather");
    }

    #[test]
    fn display_name_already_title_case() {
        assert_eq!(format_tool_display_name("Get Weather"), "Get Weather");
    }

    // ── format_tool_args_summary ─────────────────────────────────────

    #[test]
    fn args_summary_read_file() {
        let args = json!({"path": "/src/main.rs"});
        assert_eq!(
            format_tool_args_summary("read_file", &args),
            Some("/src/main.rs".into())
        );
    }

    #[test]
    fn args_summary_grep_search() {
        let args = json!({"pattern": "TODO", "path": "/src"});
        assert_eq!(
            format_tool_args_summary("grep_search", &args),
            Some("\"TODO\" in /src".into())
        );
    }

    #[test]
    fn args_summary_grep_search_no_path() {
        let args = json!({"pattern": "TODO"});
        assert_eq!(
            format_tool_args_summary("grep_search", &args),
            Some("\"TODO\" in .".into())
        );
    }

    #[test]
    fn args_summary_list_directory() {
        let args = json!({"path": "/src"});
        assert_eq!(
            format_tool_args_summary("list_directory", &args),
            Some("/src".into())
        );
    }

    #[test]
    fn args_summary_get_current_time() {
        let args = json!({"timezone": "UTC"});
        assert_eq!(
            format_tool_args_summary("get_current_time", &args),
            Some("UTC".into())
        );
    }

    #[test]
    fn args_summary_generic_fallback() {
        let args = json!({"query": "rust async"});
        assert_eq!(
            format_tool_args_summary("custom_search", &args),
            Some("rust async".into())
        );
    }

    #[test]
    fn args_summary_no_string_values() {
        let args = json!({"count": 5});
        assert_eq!(format_tool_args_summary("some_tool", &args), None);
    }

    #[test]
    fn args_summary_null_args() {
        assert_eq!(
            format_tool_args_summary("some_tool", &serde_json::Value::Null),
            None
        );
    }
}
