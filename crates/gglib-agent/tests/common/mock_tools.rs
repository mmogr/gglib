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
//! let call_log = executor.call_log_handle();  // clone before Arc wrapping
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

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
#[allow(dead_code)] // Fail/Error are API surface for future tests
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
    /// Returns a `ToolResult` with `success = false`.
    ///
    /// This is a domain-level failure — the loop feeds the message back to
    /// the LLM so it can observe and react to the failure.
    Fail {
        /// Error description returned as the tool content.
        message: String,
    },
    /// Returns `Err(anyhow::Error)`, simulating an infrastructure failure.
    ///
    /// The agent loop converts this into a `ToolResult { success: false }`.
    Error {
        /// Human-readable error description.
        message: String,
    },
}

// =============================================================================
// CallLog handle — sharable snapshot accessor
// =============================================================================

/// A clonable handle to the shared call-log, so callers can capture it before
/// the executor is wrapped in an `Arc<dyn ToolExecutorPort>`.
///
/// ```rust,ignore
/// let executor = MockToolExecutorPort::new().with_tool(…);
/// let log = executor.call_log_handle();
/// let agent = AgentLoop::new(llm, Arc::new(executor));
/// agent.run(…).await.unwrap();
///
/// let calls = log.snapshot().await;
/// assert_eq!(calls.len(), 1);
/// ```
#[derive(Clone)]
pub struct CallLogHandle(Arc<Mutex<Vec<(String, serde_json::Value)>>>);

impl CallLogHandle {
    /// Return a cloned snapshot of all `(tool_name, arguments)` pairs recorded
    /// up to now.
    pub async fn snapshot(&self) -> Vec<(String, serde_json::Value)> {
        self.0.lock().await.clone()
    }
}

// =============================================================================
// MockToolExecutorPort
// =============================================================================

/// Mock [`ToolExecutorPort`] with configurable per-tool behaviour.
///
/// All invocations are appended to a shared [`CallLogHandle`] that can be
/// captured before wrapping the executor in `Arc<dyn ToolExecutorPort>`.
pub struct MockToolExecutorPort {
    tools: Vec<ToolDefinition>,
    behaviors: HashMap<String, MockToolBehavior>,
    call_log: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
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
    pub fn with_tool(mut self, definition: ToolDefinition, behavior: MockToolBehavior) -> Self {
        self.behaviors.insert(definition.name.clone(), behavior);
        self.tools.push(definition);
        self
    }

    /// Return a [`CallLogHandle`] that shares the internal call-log.
    ///
    /// Clone this **before** wrapping the executor in `Arc<dyn ToolExecutorPort>`
    /// so you can inspect calls after the agent loop has run.
    pub fn call_log_handle(&self) -> CallLogHandle {
        CallLogHandle(Arc::clone(&self.call_log))
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
        let start = Instant::now();

        // Record the invocation.
        self.call_log
            .lock()
            .await
            .push((call.name.clone(), call.arguments.clone()));

        let behavior =
            self.behaviors
                .get(&call.name)
                .cloned()
                .unwrap_or(MockToolBehavior::Immediate {
                    content: "ok".into(),
                });

        match behavior {
            MockToolBehavior::Immediate { content } => Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content,
                success: true,
                duration_ms: start.elapsed().as_millis() as u64,
            }),
            MockToolBehavior::Delayed { millis, content } => {
                tokio::time::sleep(std::time::Duration::from_millis(millis)).await;
                Ok(ToolResult {
                    tool_call_id: call.id.clone(),
                    content,
                    success: true,
                    duration_ms: start.elapsed().as_millis() as u64,
                })
            }
            MockToolBehavior::Fail { message } => Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content: message,
                success: false,
                duration_ms: start.elapsed().as_millis() as u64,
            }),
            MockToolBehavior::Error { message } => Err(anyhow::anyhow!(message)),
        }
    }
}
