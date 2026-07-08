//! Scoring tool executor: a local `ToolExecutorPort` used only to observe
//! what tools a candidate's sampling settings cause the model to call.
//!
//! This is **not** a test mock — it lives in the production binary and is
//! used by this crate's tune orchestration (`super::run_tune`) to drive the
//! real `AgentLoop` without depending on any real MCP server. It advertises
//! exactly the tools a `TuneTask` declares, records every call the model
//! makes, and returns a deterministic synthetic success result so
//! multi-turn tasks can proceed without needing per-tool behavior scripting.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::{ToolCall, ToolDefinition, ToolResult};
use gglib_core::ports::ToolExecutorPort;
use tokio::sync::Mutex;

/// A [`ToolExecutorPort`] that records calls instead of executing them.
///
/// Construct one per task evaluation (not shared across tasks/candidates) so
/// the call log always reflects exactly one agent-loop run.
pub struct ScoringToolExecutorPort {
    tools: Vec<ToolDefinition>,
    call_log: Arc<Mutex<Vec<ToolCall>>>,
}

impl ScoringToolExecutorPort {
    /// Create a new scoring executor advertising `tools` to the agent loop.
    #[must_use]
    pub fn new(tools: Vec<ToolDefinition>) -> Self {
        Self {
            tools,
            call_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Clone the shared call-log handle so the caller can inspect recorded
    /// calls after the agent loop finishes (whether it succeeded or errored).
    #[must_use]
    pub fn call_log_handle(&self) -> Arc<Mutex<Vec<ToolCall>>> {
        Arc::clone(&self.call_log)
    }
}

#[async_trait]
impl ToolExecutorPort for ScoringToolExecutorPort {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.clone()
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult, anyhow::Error> {
        self.call_log.lock().await.push(call.clone());
        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            content: format!(r#"{{"status":"ok","tool":"{}"}}"#, call.name),
            success: true,
        })
    }
}
