//! [`CombinedToolExecutor`] — routes tool calls to the correct executor based
//! on the tool-name prefix.
//!
//! | Prefix        | Executor                   |
//! |---------------|----------------------------|
//! | `"builtin:"`  | [`BuiltinToolExecutorAdapter`] |
//! | `"{int}:"`    | [`McpToolExecutorAdapter`]     |
//!
//! The `:` separator is unambiguous in both cases: MCP tool names are
//! `[a-zA-Z0-9_-]+` and cannot contain `:`.

use std::sync::Arc;

use anyhow::anyhow;
use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ToolCall, ToolDefinition, ToolResult};

use crate::builtin::{BUILTIN_PREFIX, BuiltinToolExecutorAdapter};
use crate::service::McpService;
use crate::tool_executor::McpToolExecutorAdapter;

// =============================================================================
// Executor
// =============================================================================

/// Combines the built-in and MCP executors into a single [`ToolExecutorPort`].
///
/// `list_tools()` merges both tool sets.  `execute()` dispatches to the
/// appropriate executor by inspecting the `"builtin:"` prefix — no scan of
/// the tool list is required.
pub struct CombinedToolExecutor {
    builtin: BuiltinToolExecutorAdapter,
    mcp: McpToolExecutorAdapter,
}

impl CombinedToolExecutor {
    /// Wrap an existing `McpService` handle.
    pub fn new(mcp: Arc<McpService>) -> Self {
        Self {
            builtin: BuiltinToolExecutorAdapter,
            mcp: McpToolExecutorAdapter::new(mcp),
        }
    }
}

#[async_trait]
impl ToolExecutorPort for CombinedToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        let (builtin, mcp) = tokio::join!(self.builtin.list_tools(), self.mcp.list_tools());
        [builtin, mcp].concat()
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        if call.name.starts_with(BUILTIN_PREFIX) {
            self.builtin.execute(call).await
        } else if call.name.contains(':') {
            self.mcp.execute(call).await
        } else {
            Err(anyhow!(
                "tool name '{}' has no recognised prefix; \
                 expected 'builtin:<name>' or '<server_id>:<name>'",
                call.name
            ))
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_name_detected_by_prefix() {
        assert!("builtin:get_current_time".starts_with(BUILTIN_PREFIX));
        assert!(!"3:read_file".starts_with(BUILTIN_PREFIX));
    }

    #[test]
    fn mcp_name_detected_by_colon_but_not_builtin_prefix() {
        let name = "42:my_tool";
        assert!(!name.starts_with(BUILTIN_PREFIX));
        assert!(name.contains(':'));
    }
}
