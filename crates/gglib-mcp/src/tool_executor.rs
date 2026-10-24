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
//! This adapter is not constructed by the entrypoint crates. It is wrapped by
//! [`crate::combined::CombinedToolExecutor`], which `gglib-runtime`'s
//! `compose::compose_agent_loop_inner` builds:
//!
//! ```rust,ignore
//! let tool_executor: Arc<dyn ToolExecutorPort> = match sandbox_root {
//!     Some(root) => Arc::new(CombinedToolExecutor::with_sandbox(mcp, images, root)),
//!     None => Arc::new(CombinedToolExecutor::new(mcp, images)),
//! };
//! ```
//!
//! `images` is the [`AttachmentService`] a tool's images are stored
//! through ([`crate::tool_images`]).

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, anyhow};
use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::services::AttachmentService;
use gglib_core::{McpToolResult, ToolCall, ToolDefinition, ToolResult};
use tracing::warn;

use crate::builtin::parse_args;
use crate::service::McpService;
use crate::tool_images::split_content;

// =============================================================================
// Adapter
// =============================================================================

/// Implements [`ToolExecutorPort`] by delegating to a running [`McpService`].
///
/// # Thread safety
///
/// The adapter is `Send + Sync` because it only holds an `Arc<McpService>`
/// and an `Arc<AttachmentService>`, both already `Send + Sync`.
///
/// # Tool-name → server-id resolution
///
/// `execute` parses the leading integer from the qualified name
/// (`"{server_id}:{bare_name}"`) to route the call directly.  No list scan
/// is required; the server-id is encoded in the name itself.
#[derive(Clone)]
pub(crate) struct McpToolExecutorAdapter {
    mcp: Arc<McpService>,
    images: Arc<AttachmentService>,
}

impl McpToolExecutorAdapter {
    /// Wrap an existing `McpService` handle, storing the images its tools
    /// return through `images`.
    pub(crate) const fn new(mcp: Arc<McpService>, images: Arc<AttachmentService>) -> Self {
        Self { mcp, images }
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
    ///
    /// Each tool's [`ToolDefinition::server`] is its server's name, from one
    /// read of the servers; when they cannot be read, the tools are listed
    /// without it.
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        let names: HashMap<i64, String> = match self.mcp.list_servers().await {
            Ok(servers) => servers.into_iter().map(|s| (s.id, s.name)).collect(),
            Err(e) => {
                warn!("MCP servers could not be read to name their tools: {e}");
                HashMap::new()
            }
        };
        self.mcp
            .list_all_tools()
            .await
            .into_iter()
            .flat_map(|(server_id, tools)| {
                let server = names.get(&server_id).cloned();
                tools.into_iter().map(move |t| ToolDefinition {
                    name: qualified(server_id, &t.name),
                    description: t.description,
                    input_schema: t.input_schema,
                    title: t.title,
                    server: server.clone(),
                    deadline: None,
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
        let (server_id, bare_name): (i64, &str) =
            if let Some((prefix, bare)) = call.name.split_once(':') {
                let sid: i64 = prefix.parse().with_context(|| {
                    format!(
                        "qualified tool name '{}' has a non-integer server prefix",
                        call.name
                    )
                })?;
                (sid, bare)
            } else {
                return Err(anyhow!(
                    "tool name '{}' is unqualified; expected format is '<server_id>:<tool_name>' \
                 as produced by list_tools()",
                    call.name
                ));
            };

        // ---- Convert arguments Value → HashMap<String, Value> ---------------
        let arguments = parse_args(call)?;

        // ---- Execute --------------------------------------------------------
        let result = self
            .mcp
            .call_tool(server_id, bare_name, arguments)
            .await
            .map_err(|e| anyhow!("MCP call_tool failed: {e}"))?;

        Ok(tool_result(call.id.clone(), result, &self.images).await)
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// The name the agent loop knows server `server_id`'s `tool` by:
/// `"{server_id}:{tool}"`. The one naming rule, shared by
/// [`McpToolExecutorAdapter::list_tools`] and what a paired device's run is
/// offered (`opened.rs`), so the two cannot drift.
pub(crate) fn qualified(server_id: i64, tool: &str) -> String {
    format!("{server_id}:{tool}")
}

/// An MCP result as the agent's [`ToolResult`]: a success's content split
/// into text and stored images, an error's message as it came.
async fn tool_result(
    tool_call_id: String,
    result: McpToolResult,
    images: &AttachmentService,
) -> ToolResult {
    if !result.success {
        let text = result
            .error
            .unwrap_or_else(|| "tool returned an error without a message".to_owned());
        return ToolResult::text(tool_call_id, text, false);
    }
    let Some(data) = result.data else {
        return ToolResult::text(tool_call_id, "null", true);
    };
    let content = split_content(&data, images).await;
    ToolResult {
        tool_call_id,
        content: content.text,
        success: true,
        images: content.images,
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "tool_executor_tests.rs"]
mod tests;
