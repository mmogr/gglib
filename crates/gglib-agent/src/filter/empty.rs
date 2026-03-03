//! A [`ToolExecutorPort`] that exposes no tools and rejects all execution.

use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ToolCall, ToolDefinition, ToolResult};

use super::TOOL_NOT_AVAILABLE_MSG;

/// A [`ToolExecutorPort`] that exposes no tools and rejects all execution.
///
/// Used by [`super::super::agent_loop::AgentLoop::build`] when the caller
/// passes `Some(empty_set)` as the `tool_filter` argument.  An empty allowlist
/// means *"expose nothing"*, not *"expose everything"* — this executor enforces
/// that contract on both `list_tools` (LLM sees no tools) and `execute`
/// (defence in depth: any synthesised call is rejected).
#[derive(Debug, Default, Clone)]
pub struct EmptyToolExecutor;

#[async_trait]
impl ToolExecutorPort for EmptyToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        anyhow::bail!("tool '{}' {}", call.name, TOOL_NOT_AVAILABLE_MSG);
    }
}
