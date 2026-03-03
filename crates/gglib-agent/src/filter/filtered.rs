//! Decorator that restricts a [`ToolExecutorPort`] to a named allowlist.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ToolCall, ToolDefinition, ToolResult};

use super::TOOL_NOT_AVAILABLE_MSG;

/// Decorator that restricts a [`ToolExecutorPort`] to a named allowlist.
///
/// Both `list_tools` and `execute` enforce the allowlist:
/// - `list_tools` omits tools not in the allowlist so the LLM never learns
///   they exist.
/// - `execute` re-checks the name so an adversarially-prompted model cannot
///   invoke a tool by synthesising a call it was never shown.
///
/// # Name matching
///
/// The allowlist is compared against the **exact names returned by the inner
/// executor's `list_tools`**.  When the inner executor is
/// [`McpToolExecutorAdapter`], names are qualified with a server-id prefix
/// (e.g. `"3:read_file"`).  The `tool_filter` values forwarded from the
/// frontend should therefore use those same qualified names.
pub(crate) struct FilteredToolExecutor {
    inner: Arc<dyn ToolExecutorPort>,
    allowed: HashSet<String>,
}

impl FilteredToolExecutor {
    /// Wrap `inner`, exposing only tools whose names are in `allowed`.
    pub fn new(inner: Arc<dyn ToolExecutorPort>, allowed: HashSet<String>) -> Self {
        Self { inner, allowed }
    }
}

#[async_trait]
impl ToolExecutorPort for FilteredToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.inner
            .list_tools()
            .await
            .into_iter()
            .filter(|t| self.allowed.contains(&t.name))
            .collect()
    }

    /// Execute `call`, returning an error if the tool name is not in the
    /// allowlist.
    ///
    /// This is the defence-in-depth check: `list_tools` already withholds
    /// disallowed tools from the LLM, but an adversarial model might still
    /// synthesise a call by name.  Rejecting here ensures no disallowed tool
    /// can ever execute regardless of how the request was constructed.
    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        if !self.allowed.contains(&call.name) {
            anyhow::bail!("tool '{}' {}", call.name, TOOL_NOT_AVAILABLE_MSG);
        }
        self.inner.execute(call).await
    }
}
