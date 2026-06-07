//! In-memory index for progressive tool disclosure.
//!
//! # Problem
//!
//! Dumping the full JSON schema of every MCP tool to an external client (e.g.
//! VS Code Copilot) on every `tools/list` call costs 100 k+ tokens at 100+
//! tools and causes context-window timeouts.  The Progressive Disclosure
//! pattern fixes this by exposing three meta-tools instead of the full
//! registry:
//!
//! | Meta-tool         | Purpose                                        |
//! |-------------------|------------------------------------------------|
//! | `search_tools`    | Keyword search; returns lightweight summaries  |
//! | `get_tool_schema` | Lazily fetches one tool's full JSON schema     |
//! | `invoke_tool`     | Forwards execution to the upstream MCP server  |
//!
//! # Design
//!
//! `ToolIndex` is a pure-data, allocation-once structure built from the
//! complete `McpTool` list that the `McpService` already holds in memory.
//! It lives in `gglib-core` so that every frontend (CLI, Axum, Tauri) can
//! share the same type without depending on `gglib-mcp`.
//!
//! The index intentionally does **not** cache itself inside `AppState`.
//! Rebuilding from the in-memory `McpService` on each `search_tools` or
//! `get_tool_schema` call is microseconds and automatically reflects MCP
//! server start / stop events.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::McpTool;

/// Maximum number of [`ToolSummary`] entries returned by a single
/// [`ToolIndex::search`] call.
///
/// This cap exists to prevent "blank conditioning" — a failure mode observed
/// when an LLM receives more candidate tools than it can effectively
/// discriminate between.  Searches that match more than this number of tools
/// should be refined with a more specific query.
pub const SEARCH_RESULTS_CAP: usize = 30;

// ─── ToolSummary ─────────────────────────────────────────────────────────────

/// A lightweight, schema-free description of a single MCP tool.
///
/// Returned by [`ToolIndex::search`] so that an external client can discover
/// what tools exist without receiving every tool's full JSON input schema.
/// Once the client has identified the specific tool it needs, it calls
/// `get_tool_schema` with the [`tool_id`](ToolSummary::tool_id) to retrieve
/// the full schema lazily.
///
/// # Naming convention
///
/// `tool_id` uses the double-underscore qualified format
/// `"<server_name>__<tool_name>"` so that the ID is both human-readable and
/// usable as the `tool_id` argument to `get_tool_schema` and `invoke_tool`
/// without any further look-up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSummary {
    /// Qualified tool identifier: `"<server_name>__<tool_name>"`.
    ///
    /// Pass this string directly to `get_tool_schema` or `invoke_tool`.
    pub tool_id: String,

    /// One-line human-readable description, or an empty string if the
    /// upstream MCP server did not provide one.
    pub description: String,
}

// ─── ToolIndex ───────────────────────────────────────────────────────────────

/// An in-memory index over all tools from all running MCP servers.
///
/// Build one via [`ToolIndex::from_tools`], then use [`search`] and
/// [`get_schema`] to serve progressive-disclosure meta-tool calls without
/// exposing the full registry to external clients.
///
/// [`search`]: ToolIndex::search
/// [`get_schema`]: ToolIndex::get_schema
#[derive(Debug, Default)]
pub struct ToolIndex {
    /// Keyed by `"<server_name>__<tool_name>"`.
    by_id: HashMap<String, McpTool>,
}

impl ToolIndex {
    /// Build a `ToolIndex` from an iterator of `(qualified_id, tool)` pairs.
    ///
    /// The caller is responsible for constructing `qualified_id` in the format
    /// `"<server_name>__<tool_name>"`.  See [`build_tool_index`] in
    /// `gglib-proxy` for the canonical construction path.
    ///
    /// Duplicate IDs are silently overwritten by the last occurrence (mirrors
    /// the behaviour of the MCP server registry, which does not permit two
    /// servers with the same name to be active simultaneously).
    pub fn from_tools(iter: impl IntoIterator<Item = (String, McpTool)>) -> Self {
        Self {
            by_id: iter.into_iter().collect(),
        }
    }

    /// Search the index by keyword and return at most [`SEARCH_RESULTS_CAP`]
    /// lightweight [`ToolSummary`] entries.
    ///
    /// # Matching
    ///
    /// Both the `tool_id` and the tool's `description` field are searched
    /// using case-insensitive substring matching.  A tool is included if the
    /// lowercased `query` appears anywhere in either field.
    ///
    /// An **empty `query`** returns the first `SEARCH_RESULTS_CAP` tools in
    /// an unspecified (but deterministic within a single process run) order —
    /// useful for an LLM that wants a broad overview before deciding what to
    /// fetch.
    ///
    /// # Hard cap
    ///
    /// At most [`SEARCH_RESULTS_CAP`] results are returned regardless of how
    /// many tools match.  This is intentional: searches that return too many
    /// results should be narrowed with a more specific keyword.
    pub fn search(&self, query: &str) -> Vec<ToolSummary> {
        let q = query.to_lowercase();
        self.by_id
            .iter()
            .filter(|(id, tool)| {
                if q.is_empty() {
                    return true;
                }
                let desc = tool.description.as_deref().unwrap_or("").to_lowercase();
                id.to_lowercase().contains(&q) || desc.contains(&q)
            })
            .take(SEARCH_RESULTS_CAP)
            .map(|(id, tool)| ToolSummary {
                tool_id: id.clone(),
                description: tool.description.clone().unwrap_or_default(),
            })
            .collect()
    }

    /// Retrieve the full JSON input schema for a single tool by its qualified
    /// `tool_id`.
    ///
    /// Returns `None` when no tool with that ID exists in the index, or when
    /// the tool exists but its upstream server did not expose a schema.
    ///
    /// # Token cost
    ///
    /// Calling this for a single tool is the central efficiency gain of the
    /// Progressive Disclosure pattern — the client pays only for the one
    /// schema it actually needs, not for all schemas in the registry.
    pub fn get_schema(&self, tool_id: &str) -> Option<&serde_json::Value> {
        self.by_id
            .get(tool_id)
            .and_then(|t| t.input_schema.as_ref())
    }

    /// Returns `true` if the index contains a tool with the given `tool_id`.
    ///
    /// Used by `invoke_tool` to validate the ID before attempting resolution
    /// against the live MCP service.
    pub fn contains(&self, tool_id: &str) -> bool {
        self.by_id.contains_key(tool_id)
    }

    /// Total number of tools in the index across all MCP servers.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Returns `true` if no tools are currently indexed (no MCP servers
    /// running or all servers have zero tools).
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn make_tool(description: Option<&str>, schema: Option<serde_json::Value>) -> McpTool {
        let mut t = McpTool::new("dummy");
        if let Some(d) = description {
            t = t.with_description(d);
        }
        if let Some(s) = schema {
            t = t.with_input_schema(s);
        }
        t
    }

    fn sample_index() -> ToolIndex {
        ToolIndex::from_tools([
            (
                "files__read_file".to_string(),
                make_tool(
                    Some("Read the contents of a file"),
                    Some(json!({"type": "object", "properties": {"path": {"type": "string"}}})),
                ),
            ),
            (
                "files__write_file".to_string(),
                make_tool(Some("Write text to a file"), None),
            ),
            (
                "search__web_search".to_string(),
                make_tool(Some("Search the web"), Some(json!({}))),
            ),
            ("no_desc__tool".to_string(), make_tool(None, None)),
        ])
    }

    #[test]
    fn search_by_keyword_in_id() {
        let idx = sample_index();
        let results = idx.search("web");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_id, "search__web_search");
    }

    #[test]
    fn search_by_keyword_in_description() {
        let idx = sample_index();
        let results = idx.search("contents");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_id, "files__read_file");
    }

    #[test]
    fn search_case_insensitive() {
        let idx = sample_index();
        let results = idx.search("FILE");
        // matches "files__read_file", "files__write_file" (id), and "Read the contents of a FILE" (desc)
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.tool_id == "files__read_file"));
    }

    #[test]
    fn empty_query_returns_all_up_to_cap() {
        let idx = sample_index();
        let results = idx.search("");
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn search_respects_cap() {
        // Build an index with SEARCH_RESULTS_CAP + 5 tools.
        let tools: Vec<(String, McpTool)> = (0..SEARCH_RESULTS_CAP + 5)
            .map(|i| (format!("srv__tool_{i}"), make_tool(Some("match"), None)))
            .collect();
        let idx = ToolIndex::from_tools(tools);
        let results = idx.search("match");
        assert_eq!(results.len(), SEARCH_RESULTS_CAP);
    }

    #[test]
    fn get_schema_returns_schema() {
        let idx = sample_index();
        let schema = idx.get_schema("files__read_file");
        assert!(schema.is_some());
    }

    #[test]
    fn get_schema_missing_tool_returns_none() {
        let idx = sample_index();
        assert!(idx.get_schema("nonexistent__tool").is_none());
    }

    #[test]
    fn get_schema_tool_without_schema_returns_none() {
        let idx = sample_index();
        assert!(idx.get_schema("files__write_file").is_none());
    }

    #[test]
    fn contains_and_len() {
        let idx = sample_index();
        assert!(idx.contains("files__read_file"));
        assert!(!idx.contains("nothing__here"));
        assert_eq!(idx.len(), 4);
        assert!(!idx.is_empty());
    }

    #[test]
    fn empty_index_is_empty() {
        let idx = ToolIndex::default();
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
        assert!(idx.search("anything").is_empty());
        assert!(idx.get_schema("anything").is_none());
    }
}
