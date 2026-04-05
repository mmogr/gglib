//! Tool-result summary formatters for CLI output.
//!
//! Builtin filesystem tools (`read_file`, `list_directory`, `grep_search`) get
//! compact, human-readable summaries.  Everything else falls back to a
//! truncated preview via [`truncate_string`].

use gglib_core::domain::agent::ToolResult;

use crate::presentation::tables::truncate_string;

/// Format a tool result with a tool-specific summary instead of a generic
/// truncation.  Builtin filesystem tools get richer output; everything else
/// falls back to the standard 80-char preview.
pub(super) fn format_tool_result(tool_name: &str, result: &ToolResult) -> String {
    if !result.success {
        return truncate_string(&result.content, 120);
    }

    match tool_name {
        "builtin:read_file" => format_read_file(&result.content),
        "builtin:list_directory" => format_list_directory(&result.content),
        "builtin:grep_search" => format_grep_search(&result.content),
        "builtin:get_current_time" => format_get_current_time(&result.content),
        _ => truncate_string(&result.content, 80),
    }
}

/// `read_file` → show line count and a compact preview of the first few lines.
fn format_read_file(content: &str) -> String {
    let line_count = content.lines().count();
    let truncated = content.contains("[truncated");

    let first_line = content.lines().next().unwrap_or("");
    let preview = truncate_string(first_line, 50);

    if truncated {
        format!("{line_count}+ lines (truncated)  {preview}")
    } else {
        format!("{line_count} lines  {preview}")
    }
}

/// `list_directory` → show entry count.
fn format_list_directory(content: &str) -> String {
    if content == "(empty directory)" {
        return "(empty directory)".to_string();
    }
    let count = content.lines().count();
    let dirs = content.lines().filter(|l| l.ends_with('/')).count();
    let files = count - dirs;
    format!("{count} entries ({files} files, {dirs} dirs)")
}

/// `grep_search` → show match count and first match preview.
fn format_grep_search(content: &str) -> String {
    if content.starts_with("no matches") {
        return "no matches".to_string();
    }
    let match_count = content
        .lines()
        .filter(|l| !l.starts_with("[results"))
        .count();
    let truncated = content.contains("[results truncated");
    let first = content.lines().next().unwrap_or("");
    let preview = truncate_string(first, 50);

    if truncated {
        format!("{match_count}+ matches  {preview}")
    } else {
        format!("{match_count} matches  {preview}")
    }
}

/// `get_current_time` → extract the readable time value from result JSON.
fn format_get_current_time(content: &str) -> String {
    // The tool returns JSON like {"timezone":"UTC","datetime":"2025-01-15T10:30:00Z","unix_timestamp":...}
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(content) {
        if let Some(dt) = v.get("datetime").and_then(|d| d.as_str()) {
            let tz = v.get("timezone").and_then(|t| t.as_str()).unwrap_or("");
            if tz.is_empty() {
                return dt.to_string();
            }
            return format!("{dt} ({tz})");
        }
    }
    // Fallback: content is already human-readable or unexpected format.
    truncate_string(content, 80)
}
