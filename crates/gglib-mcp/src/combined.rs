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
use gglib_core::domain::agent::ToolProgressSink;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::services::AttachmentService;
use gglib_core::{ToolCall, ToolDefinition, ToolResult};

use crate::builtin::{BUILTIN_PREFIX, BuiltinToolExecutorAdapter};
use crate::service::McpService;
use crate::tool_executor::McpToolExecutorAdapter;

// =============================================================================
// Executor
// =============================================================================

/// Combines the built-in and MCP executors into a single [`ToolExecutorPort`].
///
/// `list_tools()` merges both tool sets.  `execute()` and
/// `execute_with_progress()` dispatch to the appropriate executor by
/// inspecting the `"builtin:"` prefix — no scan of the tool list is required.
pub struct CombinedToolExecutor {
    builtin: Arc<dyn ToolExecutorPort>,
    mcp: Arc<dyn ToolExecutorPort>,
}

impl CombinedToolExecutor {
    /// Wrap an existing `McpService` handle, storing the images its tools
    /// return through `images`.
    pub fn new(mcp: Arc<McpService>, images: Arc<AttachmentService>) -> Self {
        Self {
            builtin: Arc::new(BuiltinToolExecutorAdapter::default()),
            mcp: Arc::new(McpToolExecutorAdapter::new(mcp, images)),
        }
    }

    /// As [`Self::new`], with filesystem tools sandboxed to `root`.
    pub fn with_sandbox(
        mcp: Arc<McpService>,
        images: Arc<AttachmentService>,
        root: std::path::PathBuf,
    ) -> Self {
        Self {
            builtin: Arc::new(BuiltinToolExecutorAdapter::with_sandbox(root)),
            mcp: Arc::new(McpToolExecutorAdapter::new(mcp, images)),
        }
    }

    /// As [`Self::new`], with the builtins `builtin` offers: a sandbox, a
    /// drawing tool, or both.
    pub fn with_builtin(
        mcp: Arc<McpService>,
        images: Arc<AttachmentService>,
        builtin: BuiltinToolExecutorAdapter,
    ) -> Self {
        Self {
            builtin: Arc::new(builtin),
            mcp: Arc::new(McpToolExecutorAdapter::new(mcp, images)),
        }
    }

    /// The executor a call goes to, by its name's prefix.
    fn route(&self, call: &ToolCall) -> anyhow::Result<&dyn ToolExecutorPort> {
        if call.name.starts_with(BUILTIN_PREFIX) {
            Ok(self.builtin.as_ref())
        } else if call.name.contains(':') {
            Ok(self.mcp.as_ref())
        } else {
            Err(anyhow!(
                "tool name '{}' has no recognised prefix; \
                 expected 'builtin:<name>' or '<server_id>:<name>'",
                call.name
            ))
        }
    }
}

#[async_trait]
impl ToolExecutorPort for CombinedToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        let (builtin, mcp) = tokio::join!(self.builtin.list_tools(), self.mcp.list_tools());
        builtin.into_iter().chain(mcp).collect()
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        self.route(call)?.execute(call).await
    }

    /// Routed as [`execute`](Self::execute), keeping `sink`, so a builtin
    /// that reports progress is heard through this executor and any filter
    /// over it. MCP tools keep the default and report nothing.
    async fn execute_with_progress(
        &self,
        call: &ToolCall,
        sink: &dyn ToolProgressSink,
    ) -> anyhow::Result<ToolResult> {
        self.route(call)?.execute_with_progress(call, sink).await
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "combined_progress_tests.rs"]
mod progress_tests;

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
