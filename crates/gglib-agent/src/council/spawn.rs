//! Dynamic sub-team spawning support for the orchestrator.
//!
//! A worker node may call the built-in `spawn_subteam` tool to request that
//! the executor dynamically plan and execute a subordinate team for a
//! focused sub-goal.  The mechanism has two parts:
//!
//! 1. [`spawn_subteam_tool_def`] — the [`ToolDefinition`] injected into every
//!    worker's tool list.  The LLM invokes it with `{"goal": "…",
//!    "suggested_roles": ["role-a", "role-b"]}`.
//!
//! 2. [`SpawnCapturingExecutor`] — a transparent [`ToolExecutorPort`] decorator
//!    that intercepts `spawn_subteam` calls, writes the parsed [`SpawnRequest`]
//!    into a shared [`SpawnSink`], and returns a synthetic
//!    `ToolResult { success: true }` so the LLM can finish its turn normally.
//!    After the worker loop terminates, the executor reads the sink and drives
//!    the approval + planning + recursive execution flow.
//!
//! # Name
//!
//! The tool is deliberately named `"spawn_subteam"` (no prefix) to keep it
//! concise; it is always injected directly by the executor and is never
//! routed through the MCP adapter.

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::Mutex;

use gglib_core::domain::agent::{ToolCall, ToolDefinition, ToolResult};
use gglib_core::ports::ToolExecutorPort;

// =============================================================================
// Tool name constant
// =============================================================================

/// Wire name used both in the tool definition and in the executor interceptor.
pub const SPAWN_SUBTEAM_TOOL_NAME: &str = "spawn_subteam";

// =============================================================================
// SpawnRequest
// =============================================================================

/// A request to dynamically plan and execute a sub-team for `goal`.
///
/// Populated by [`SpawnCapturingExecutor`] when a worker calls `spawn_subteam`,
/// then read by the executor after the worker terminates.
#[derive(Debug, Clone)]
pub struct SpawnRequest {
    /// The sub-goal to hand off to the spawned team.
    pub goal: String,
    /// Role hints the requesting worker proposed for the new team.
    pub suggested_roles: Vec<String>,
}

// =============================================================================
// SpawnSink
// =============================================================================

/// A shared, once-writable slot for a [`SpawnRequest`].
///
/// Each worker node gets its own fresh `SpawnSink`.  The
/// [`SpawnCapturingExecutor`] fills it on the first `spawn_subteam` call;
/// subsequent calls in the same turn are silently ignored (only one spawn per
/// node is supported).
pub type SpawnSink = Arc<Mutex<Option<SpawnRequest>>>;

// =============================================================================
// Tool definition
// =============================================================================

/// Returns the [`ToolDefinition`] for `spawn_subteam` that is injected into
/// every worker's tool list by the executor.
///
/// # Schema
///
/// ```json
/// {
///   "type": "object",
///   "properties": {
///     "goal": { "type": "string" },
///     "suggested_roles": { "type": "array", "items": { "type": "string" } }
///   },
///   "required": ["goal"]
/// }
/// ```
#[must_use]
pub fn spawn_subteam_tool_def() -> ToolDefinition {
    ToolDefinition {
        name: SPAWN_SUBTEAM_TOOL_NAME.to_string(),
        description: Some(
            "Request that the orchestrator dynamically plan and execute a focused sub-team for a \
             specific sub-goal. Use when the current task requires a coordinated effort that \
             exceeds your scope. Provide a clear goal and any role hints that would help the \
             planner assemble the right team."
                .to_string(),
        ),
        input_schema: Some(serde_json::json!({
            "type": "object",
            "properties": {
                "goal": {
                    "type": "string",
                    "description": "The focused sub-goal for the spawned team to accomplish."
                },
                "suggested_roles": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional role hints (e.g. [\"researcher\", \"writer\"]) to \
                                    guide the planner when assembling the team."
                }
            },
            "required": ["goal"]
        })),
        title: Some("Spawn Sub-team".to_string()),
    }
}

// =============================================================================
// SpawnCapturingExecutor
// =============================================================================

/// A [`ToolExecutorPort`] decorator that intercepts `spawn_subteam` calls.
///
/// All other tool calls are forwarded unchanged to the wrapped `inner`
/// executor.  When `spawn_subteam` is invoked the arguments are parsed and
/// written to `sink`, and a synthetic success result is returned so the LLM
/// can complete its turn without an error.
pub struct SpawnCapturingExecutor {
    inner: Arc<dyn ToolExecutorPort>,
    sink: SpawnSink,
}

impl SpawnCapturingExecutor {
    /// Create a new decorator around `inner` that captures spawn requests into
    /// `sink`.
    pub fn new(inner: Arc<dyn ToolExecutorPort>, sink: SpawnSink) -> Self {
        Self { inner, sink }
    }
}

/// Arguments accepted by the `spawn_subteam` tool.
#[derive(Debug, Deserialize)]
struct SpawnArgs {
    goal: String,
    #[serde(default)]
    suggested_roles: Vec<String>,
}

#[async_trait]
impl ToolExecutorPort for SpawnCapturingExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        let mut tools = self.inner.list_tools().await;
        tools.push(spawn_subteam_tool_def());
        tools
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult, anyhow::Error> {
        if call.name != SPAWN_SUBTEAM_TOOL_NAME {
            return self.inner.execute(call).await;
        }

        // Parse the spawn arguments (best-effort; missing goal → empty string).
        let args: SpawnArgs = serde_json::from_value(call.arguments.clone()).unwrap_or(SpawnArgs {
            goal: String::new(),
            suggested_roles: vec![],
        });

        // Write to sink only on the first call per worker.
        {
            let mut guard = self.sink.lock().await;
            if guard.is_none() {
                *guard = Some(SpawnRequest {
                    goal: args.goal,
                    suggested_roles: args.suggested_roles,
                });
            }
        }

        Ok(ToolResult {
            tool_call_id: call.id.clone(),
            content: "Sub-team spawn requested. The orchestrator will plan and execute a \
                      dedicated team for this goal. Continue with your response."
                .to_string(),
            success: true,
        })
    }
}
