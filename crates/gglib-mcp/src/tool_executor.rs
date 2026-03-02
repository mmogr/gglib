//! [`ToolExecutorPort`] adapter backed by [`McpService`].
//!
//! This module is the sole infrastructure seam between the agent domain and the
//! MCP server layer.  It lives here — in `gglib-mcp` — because it references the
//! MCP internals (`McpService`, `McpTool`, `McpToolResult`).  The `gglib-agent`
//! orchestration crate is deliberately kept free of any MCP dependency; it only
//! accepts the abstract `Arc<dyn ToolExecutorPort>`.
//!
//! # Tool-name format
//!
//! Tool names are always **qualified**: `"{server_id}:{bare_name}"` (e.g.
//! `"3:read_file"`).  This is the format produced by [`McpToolExecutorAdapter::list_tools`]
//! and the only format accepted by [`McpToolExecutorAdapter::execute`].  Bare,
//! unqualified names are rejected at execution time with a descriptive error.
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

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ToolCall, ToolDefinition, ToolResult};

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
/// `execute` parses the leading integer from the qualified name
/// (`"{server_id}:{bare_name}"`) to route the call directly.  No list scan
/// is required; the server-id is encoded in the name itself.
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
    /// Tool names are qualified with `{server_id}:` (e.g. `"3:read_file"`)
    /// to guarantee uniqueness when multiple servers expose tools with the
    /// same bare name.  `execute()` requires this qualified format.
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.mcp
            .list_all_tools()
            .await
            .into_iter()
            .flat_map(|(server_id, tools)| {
                tools.into_iter().map(move |t| ToolDefinition {
                    name: format!("{server_id}:{}", t.name),
                    description: t.description,
                    input_schema: t.input_schema,
                })
            })
            .collect()
    }

    /// Execute a single tool call, returning a [`ToolResult`].
    ///
    /// `call.name` **must** be a qualified name of the form
    /// `"{server_id}:{bare_tool_name}"` as produced by [`Self::list_tools`].
    /// Unqualified names are rejected with an error.
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
        // Only qualified names are accepted: "{server_id}:{tool_name}".
        // The `:` separator is unambiguous because MCP tool names
        // (`[a-zA-Z0-9_-]+`) cannot contain `:`.
        let (server_id, bare_name): (i64, &str) = if let Some((prefix, bare)) =
            call.name.split_once(':')
        {
            let sid: i64 = prefix.parse().with_context(|| {
                format!(
                    "qualified tool name '{}' has a non-integer server prefix",
                    call.name
                )
            })?;
            (sid, bare)
        } else {
            return Err(anyhow!(
                "tool name '{}' is unqualified; expected format is '{{server_id}}:{{tool_name}}' \
                 as produced by list_tools()",
                call.name
            ));
        };

        // ---- Convert arguments Value → HashMap<String, Value> ---------------
        let arguments: HashMap<String, serde_json::Value> = match &call.arguments {
            serde_json::Value::Object(map) => map.clone().into_iter().collect(),
            serde_json::Value::Null => HashMap::new(),

            other => {
                return Err(anyhow!(
                    "tool '{}' arguments must be a JSON object; got {}",
                    call.name,
                    other
                ));
            }
        };

        // ---- Execute --------------------------------------------------------
        let result = self
            .mcp
            .call_tool(server_id, bare_name, arguments)
            .await
            .map_err(|e| anyhow!("MCP call_tool failed: {e}"))?;

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

    #[test]
    fn qualified_name_splits_on_colon_separator() {
        // execute() uses split_once(':') to parse the qualified form
        // "{server_id}:{bare_name}".  `:` is not permitted in MCP tool names
        // (`[a-zA-Z0-9_-]+`), making it an unambiguous separator.

        let (prefix, bare) = "3:search".split_once(':').unwrap();
        assert_eq!(prefix.parse::<i64>().unwrap(), 3i64);
        assert_eq!(bare, "search");

        let (p, b) = "7:read_file".split_once(':').unwrap();
        assert_eq!(p.parse::<i64>().unwrap(), 7i64);
        assert_eq!(b, "read_file");
    }

    #[test]
    fn non_integer_server_prefix_is_an_error() {
        // A name that contains `:` but whose prefix is not a valid i64 must
        // cause execute() to return Err rather than panic or dispatch blindly.
        let name = "not_int:tool";
        let (prefix, _) = name.split_once(':').unwrap();
        assert!(
            prefix.parse::<i64>().is_err(),
            "non-integer prefix must fail to parse as server id"
        );
    }

    #[test]
    fn unqualified_name_is_rejected() {
        // Bare tool names (no `:`) are not supported; callers must always use
        // names as returned by list_tools() which includes the server-id prefix.
        let name = "search";
        assert!(
            name.split_once(':').is_none(),
            "unqualified name must not contain a colon — execute() will reject it"
        );
    }
}
