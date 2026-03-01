//! Mock implementation of [`ToolExecutorPort`] for integration testing.
//!
//! Allows configuring per-tool behaviour and records all invocations so tests
//! can assert on which tools were called and with what arguments.
//!
//! ```rust,ignore
//! let executor = MockToolExecutorPort::new()
//!     .with_tool(
//!         ToolDefinition::new("search"),
//!         MockToolBehavior::Immediate { content: "results…".into() },
//!     )
//!     .with_tool(
//!         ToolDefinition::new("slow_io"),
//!         MockToolBehavior::Delayed { millis: 5_000, content: "ok".into() },
//!     );
//! let call_log = Arc::clone(&executor.call_log);  // clone before Arc wrapping
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use gglib_core::domain::agent::{ToolCall, ToolDefinition, ToolResult};
use gglib_core::ports::ToolExecutorPort;
use tokio::sync::Mutex;

// =============================================================================
// MockToolBehavior
// =============================================================================

/// Configurable response strategy for a single mock tool.
#[derive(Clone)]
pub enum MockToolBehavior {
    /// Returns a successful result immediately.
    Immediate {
        /// Content returned to the LLM as the tool output.
        content: String,
    },
    /// Sleeps for `millis` milliseconds before returning a successful result.
    ///
    /// Useful for exercising the per-tool timeout in [`AgentConfig`].
    Delayed {
        /// How long to sleep before returning, in milliseconds.
        millis: u64,
        /// Content returned to the LLM as the tool output.
        content: String,
    },
    /// Returns `Err(anyhow::Error)`, simulating an infrastructure failure.
    ///
    /// The agent loop converts this into a `ToolResult { success: false }`.
    Error {
        /// Human-readable error description.
        message: String,
    },
    /// Returns `Ok(ToolResult { success: false, … })`, simulating a tool that
    /// ran successfully but reported a logical failure.
    ///
    /// Use this to test the path where the LLM receives a failed tool result
    /// and can reason about / retry it, distinct from infrastructure errors.
    Failure {
        /// Content of the tool failure message fed back to the LLM.
        content: String,
    },
}

// =============================================================================
// CallLog handle — sharable snapshot accessor
// =============================================================================

// =============================================================================
// MockToolExecutorPort
// =============================================================================

/// Mock [`ToolExecutorPort`] with configurable per-tool behaviour.
///
/// All invocations are recorded in the public `call_log` field — clone the
/// inner `Arc` before wrapping the executor in `Arc<dyn ToolExecutorPort>` so
/// you can inspect it after the agent has run.
pub struct MockToolExecutorPort {
    tools: Vec<ToolDefinition>,
    behaviors: HashMap<String, MockToolBehavior>,
    /// Shared call log — clone the `Arc` before wrapping in `Arc<dyn …>`.
    pub call_log: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
}

impl MockToolExecutorPort {
    /// Create an empty mock with no tools configured.
    pub fn new() -> Self {
        Self {
            tools: Vec::new(),
            behaviors: HashMap::new(),
            call_log: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Register a tool with its associated behaviour (builder-style).
    ///
    /// Tools are advertised to the LLM in the order they are added.
    #[must_use]
    pub fn with_tool(mut self, definition: ToolDefinition, behavior: MockToolBehavior) -> Self {
        self.behaviors.insert(definition.name.clone(), behavior);
        self.tools.push(definition);
        self
    }
}

impl Default for MockToolExecutorPort {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutorPort for MockToolExecutorPort {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.clone()
    }

    async fn execute(&self, call: &ToolCall) -> Result<ToolResult> {
        // Record the invocation.
        self.call_log
            .lock()
            .await
            .push((call.name.clone(), call.arguments.clone()));

        let behavior =
            self.behaviors.get(&call.name).cloned().ok_or_else(|| {
                anyhow::anyhow!("MockToolExecutorPort: unknown tool '{}'", call.name)
            })?;

        match behavior {
            MockToolBehavior::Immediate { content } => Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content,
                success: true,
            }),
            MockToolBehavior::Delayed { millis, content } => {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                Ok(ToolResult {
                    tool_call_id: call.id.clone(),
                    content,
                    success: true,
                })
            }
            MockToolBehavior::Error { message } => Err(anyhow::anyhow!(message)),
            MockToolBehavior::Failure { content } => Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content,
                success: false,
            }),
        }
    }
}
