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
        return truncate_string(&result.content, 80);
    }

    match tool_name {
        "builtin:read_file" => format_read_file(&result.content),
        "builtin:list_directory" => format_list_directory(&result.content),
        "builtin:grep_search" => format_grep_search(&result.content),
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
