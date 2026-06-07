//! Progressive-disclosure adapter between the internal `McpService` registry
//! and the three meta-tools exposed to external MCP clients.
//!
//! # Why three meta-tools?
//!
//! Dumping every tool's full JSON input schema on every `tools/list` response
//! costs tens of thousands of tokens when many MCP servers are running.
//! Instead the proxy exposes exactly three stable entry points:
//!
//! | Tool              | Token cost    | Purpose                                  |
//! |-------------------|---------------|------------------------------------------|
//! | `search_tools`    | Minimal       | Keyword search; returns IDs + one-liners |
//! | `get_tool_schema` | One schema    | Lazily fetch a single tool's full schema |
//! | `invoke_tool`     | Zero overhead | Execute by qualified ID + arguments      |
//!
//! The external client pays only for the schemas it actually needs.
//!
//! # Relationship to `gglib-mcp`
//!
//! This module is a **read-only adapter**.  It calls `McpService::list_servers`,
//! `McpService::list_all_tools`, and `McpService::get_server_by_name` to build
//! and query a [`ToolIndex`].  The execution path (`invoke_tool`) delegates
//! directly to `McpService::call_tool` — the internal MCP engine is untouched.

use std::collections::HashMap;

use gglib_core::{McpTool, ToolIndex};
use gglib_mcp::McpService;
use serde_json::json;

use super::types::McpToolSpec;

// ─── Index construction ───────────────────────────────────────────────────

/// Build a [`ToolIndex`] from all tools currently registered across all
/// running MCP servers.
///
/// The `tool_id` for each entry uses the double-underscore qualified format
/// `"<server_name>__<tool_name>"` so it can be passed directly to
/// `get_tool_schema` or `invoke_tool` without any further transformation.
///
/// This function re-queries `McpService` on every call — no stale cache.
/// `list_all_tools()` reads from an in-memory map inside `McpManager` and is
/// microseconds in practice.
pub(super) async fn build_tool_index(mcp: &McpService) -> ToolIndex {
    let (flat, server_names) = build_flat_tool_list(mcp).await;
    let entries = flat.into_iter().map(|(server_id, tool)| {
        let server_name = server_names
            .get(&server_id)
            .map(String::as_str)
            .unwrap_or("unknown");
        let qualified_id = format!("{server_name}__{}", tool.name);
        (qualified_id, tool)
    });
    ToolIndex::from_tools(entries)
}

/// Resolve a `"server_name__tool_name"` qualified ID to the numeric
/// `(server_id, bare_tool_name)` pair required by `McpService::call_tool`.
///
/// Returns `None` when the ID is malformed (missing `__`), the server name
/// is not found, or the server is not currently running.
pub(super) async fn resolve_tool_name(
    mcp: &McpService,
    qualified: &str,
) -> Option<(i64, String)> {
    let (server_name, bare_name) = qualified.split_once("__")?;
    let server = mcp.get_server_by_name(server_name).await.ok()?;
    Some((server.id, bare_name.to_string()))
}

// ─── Meta-tool spec construction ──────────────────────────────────────────

/// Build the three [`McpToolSpec`] entries that are sent to external clients
/// on every `tools/list` response.
///
/// These specs are intentionally **static** — their descriptions never contain
/// dynamic content such as a capability index.  The LLM must call
/// `search_tools` to discover what is available.
///
/// The input schemas are minimal JSON Schema objects that unambiguously
/// describe each tool's parameters without token waste.
pub(super) fn meta_tools_list(_index: &ToolIndex) -> Vec<McpToolSpec> {
    vec![
        McpToolSpec {
            name: "search_tools".to_string(),
            description: Some(
                "Search the internal registry for available tools by keyword. \
                 Returns a list of matching tool IDs and short descriptions. \
                 Pass an empty string to browse all available tools (up to 30 results). \
                 Use the returned tool_id with get_tool_schema or invoke_tool."
                    .to_string(),
            ),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Keyword to search for in tool names and descriptions. Pass an empty string to list all available tools."
                    }
                },
                "required": ["query"]
            })),
            title: None,
        },
        McpToolSpec {
            name: "get_tool_schema".to_string(),
            description: Some(
                "Fetch the full JSON input schema for a specific tool by its qualified ID. \
                 Call search_tools first to discover available tool IDs."
                    .to_string(),
            ),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "tool_id": {
                        "type": "string",
                        "description": "The qualified tool ID in the format \"server_name__tool_name\", as returned by search_tools."
                    }
                },
                "required": ["tool_id"]
            })),
            title: None,
        },
        McpToolSpec {
            name: "invoke_tool".to_string(),
            description: Some(
                "Invoke a specific MCP tool by its qualified ID with the given arguments. \
                 Call get_tool_schema first to discover the required argument fields."
                    .to_string(),
            ),
            input_schema: Some(json!({
                "type": "object",
                "properties": {
                    "tool_id": {
                        "type": "string",
                        "description": "The qualified tool ID in the format \"server_name__tool_name\", as returned by search_tools."
                    },
                    "arguments": {
                        "type": "object",
                        "description": "Arguments to pass to the tool. Use get_tool_schema to discover the required fields.",
                        "additionalProperties": true
                    }
                },
                "required": ["tool_id", "arguments"]
            })),
            title: None,
        },
    ]
}

// ─── Internal helpers ─────────────────────────────────────────────────────

/// Flatten all tools from all running MCP servers into a `(server_id, McpTool)`
/// list alongside a `server_id → name` map.
///
/// This is a shared primitive used by both [`build_tool_index`] and any
/// remaining internal callers.  It performs two `McpService` calls which read
/// from in-memory state and are not expected to fail; failures produce empty
/// collections rather than propagating an error.
async fn build_flat_tool_list(
    mcp: &McpService,
) -> (Vec<(i64, McpTool)>, HashMap<i64, String>) {
    let servers = mcp.list_servers().await.unwrap_or_default();
    let server_names: HashMap<i64, String> =
        servers.iter().map(|s| (s.id, s.name.clone())).collect();

    let all_tools = mcp.list_all_tools().await;
    let flat: Vec<(i64, McpTool)> = all_tools
        .into_iter()
        .flat_map(|(sid, tools)| tools.into_iter().map(move |t| (sid, t)))
        .collect();

    (flat, server_names)
}
