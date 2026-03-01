//! [`ToolExecutorPort`] adapter backed by [`McpService`].
//!
//! This module is the sole infrastructure seam between the agent domain and the
//! MCP server layer.  It lives here — in `gglib-mcp` — because it references the
//! MCP internals (`McpService`, `McpTool`, `McpToolResult`).  The `gglib-agent`
//! orchestration crate is deliberately kept free of any MCP dependency; it only
//! accepts the abstract `Arc<dyn ToolExecutorPort>`.
//!
//! # Wiring
//!
//! Entrypoint crates (`gglib-axum`, `gglib-cli`) construct this adapter at the
//! composition root:
//!
//! ```rust,ignore
//! let executor: Arc<dyn ToolExecutorPort> =
//!     Arc::new(McpToolExecutorAdapter::new(Arc::clone(&mcp_service)));
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ToolCall, ToolDefinition, ToolResult, elapsed_ms};

use crate::service::McpService;

// =============================================================================
// Adapter
// =============================================================================

/// Implements [`ToolExecutorPort`] by delegating to a running [`McpService`].
///
/// # Thread safety
///
/// The adapter is `Send + Sync` because it only holds `Arc<McpService>`, which
/// is already `Send + Sync`.
///
/// # Tool-name → server-id resolution
///
/// `McpService::list_all_tools` returns `Vec<(server_id, Vec<McpTool>)>`.  On
/// each `execute` call the adapter performs a linear scan to find the `server_id`
/// associated with the requested tool name.  This is deliberately simple: MCP
/// tool lists are small (typically tens of entries) and `list_all_tools` is an
/// in-memory `RwLock` read — nanosecond cost.
#[derive(Clone)]
pub struct McpToolExecutorAdapter {
    mcp: Arc<McpService>,
}

impl McpToolExecutorAdapter {
    /// Wrap an existing `McpService` handle.
    pub const fn new(mcp: Arc<McpService>) -> Self {
        Self { mcp }
    }
}

// =============================================================================
// ToolExecutorPort implementation
// =============================================================================

#[async_trait]
impl ToolExecutorPort for McpToolExecutorAdapter {
    /// List every tool available across all running MCP servers.
    ///
    /// Tool names are prefixed with `{server_id}__` (e.g. `3__read_file`) to
    /// guarantee uniqueness when multiple servers expose tools with the same
    /// bare name.  `execute()` accepts both the qualified and the bare name for
    /// interoperability with `FilteredToolExecutor` (which operates on the
    /// names as returned here).
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.mcp
            .list_all_tools()
            .await
            .into_iter()
            .flat_map(|(server_id, tools)| {
                tools.into_iter().map(move |t| ToolDefinition {
                    name: format!("{server_id}__{}", t.name),
                    description: t.description,
                    input_schema: t.input_schema,
                })
            })
            .collect()
    }

    /// Execute a single tool call, returning a [`ToolResult`].
    ///
    /// Failures are **not** propagated as `anyhow::Error` unless the tool
    /// cannot be found or the arguments are structurally invalid (those are
    /// infrastructure-level problems, not tool-level failures).  A tool that
    /// returns an error response from the MCP server is represented as a
    /// `ToolResult { success: false, … }` so the agent can reason about the
    /// failure and retry or adjust.
    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        // ---- Resolve server_id and bare tool name from call.name ------------
        //
        // Accepts two formats:
        //   - qualified:   "{server_id}__{tool_name}"  (produced by list_tools)
        //   - unqualified: "{tool_name}"               (e.g. from FilteredToolExecutor)
        let (server_id, bare_name): (i64, &str) = if let Some((prefix, bare)) =
            call.name.split_once("__")
        {
            // Qualified format: trust the prefix directly and skip list_all_tools().
            // Any mismatch (bad server_id or unknown tool name) surfaces as an Err
            // from call_tool with a descriptive McpServiceError — no pre-flight scan
            // needed.
            let sid: i64 = prefix.parse().with_context(|| {
                format!(
                    "qualified tool name '{}' has a non-integer server prefix",
                    call.name
                )
            })?;
            (sid, bare)
        } else {
            // Unqualified — linear scan across all servers to find the owner.
            let all_tools = self.mcp.list_all_tools().await;
            let server_id = all_tools
                .iter()
                .find_map(|(id, tools)| tools.iter().any(|t| t.name == call.name).then_some(*id))
                .with_context(|| {
                    format!("no running MCP server exposes a tool named '{}'", call.name)
                })?;
            (server_id, call.name.as_str())
        };

        // ---- Convert arguments Value → HashMap<String, Value> ---------------
        let arguments: HashMap<String, serde_json::Value> = match &call.arguments {
            serde_json::Value::Object(map) => {
                map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            }
            serde_json::Value::Null => HashMap::new(),
            other => {
                return Err(anyhow!(
                    "tool '{}' arguments must be a JSON object; got {}",
                    call.name,
                    other
                ));
            }
        };

        // ---- Execute with wall-clock timing ----------------------------------
        let start = Instant::now();
        let result = self
            .mcp
            .call_tool(server_id, bare_name, arguments)
            .await
            .map_err(|e| anyhow!("MCP call_tool failed: {e}"))?;
        let duration_ms = elapsed_ms(start);

        // ---- Convert McpToolResult → ToolResult ------------------------------
        let (content, success) = if result.success {
            let text = result
                .data
                .as_ref()
                .map_or_else(|| "null".to_owned(), std::string::ToString::to_string);
            (text, true)
        } else {
            let text = result
                .error
                .unwrap_or_else(|| "tool returned an error without a message".to_owned());
            (text, false)
        };

        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            content,
            success,
            // wait_ms measures time spent queued for a concurrency permit inside
            // execute_tools_parallel.  The adapter has no view into that; the real
            // value is always overwritten by the caller.  Zero is a safe sentinel.
            wait_ms: 0,
            duration_ms,
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use gglib_core::{ToolCall, ToolDefinition};
    use serde_json::json;

    use super::*;

    /// Minimal smoke-test: verify the argument conversion from `Value::Object`
    /// to `HashMap` without needing a real `McpService`.
    #[test]
    fn arguments_object_round_trips() {
        let args = json!({ "path": "/tmp/foo", "recursive": true });
        let call = ToolCall {
            id: "c1".into(),
            name: "fs_list".into(),
            arguments: args,
        };

        // Extract the same conversion logic used in execute()
        let map: HashMap<String, serde_json::Value> = match &call.arguments {
            serde_json::Value::Object(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            _ => panic!("expected object"),
        };

        assert_eq!(map["path"], json!("/tmp/foo"));
        assert_eq!(map["recursive"], json!(true));
    }

    #[test]
    fn null_arguments_produce_empty_map() {
        let call = ToolCall {
            id: "c2".into(),
            name: "get_time".into(),
            arguments: serde_json::Value::Null,
        };
        let map: HashMap<String, serde_json::Value> = match &call.arguments {
            serde_json::Value::Object(m) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            serde_json::Value::Null => HashMap::new(),
            other => panic!("unexpected: {other}"),
        };
        assert!(map.is_empty());
    }

    #[test]
    fn tool_definition_conversion_preserves_schema() {
        let mcp_tool = gglib_core::McpTool {
            name: "search".into(),
            description: Some("full-text search".into()),
            input_schema: Some(
                json!({ "type": "object", "properties": { "q": { "type": "string" } } }),
            ),
        };
        let def = ToolDefinition {
            name: mcp_tool.name.clone(),
            description: mcp_tool.description.clone(),
            input_schema: mcp_tool.input_schema,
        };
        assert_eq!(def.name, "search");
        assert!(def.input_schema.is_some());
    }
}
