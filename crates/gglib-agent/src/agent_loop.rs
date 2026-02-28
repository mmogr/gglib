//! [`AgentLoopPort`] implementation: the main LLM→tool→LLM state machine.
//!
//! This module wires all the utilities together:
//!
//! ```text
//! AgentLoop::run()
//!   │
//!   ├─ context_pruning::prune_for_budget()        budget management
//!   ├─ tool_executor.list_tools()                 tool schema discovery
//!   ├─ llm.chat_stream()                          LLM call (streaming)
//!   ├─ stream_collector::collect_stream()          text forwarded live ──→ AgentEvent::TextDelta
//!   ├─ stagnation::StagnationDetector::record()   stagnation guard ──→ AgentEvent::Error (on failure)
//!   ├─ loop_detection::LoopDetector::check()      loop guard       ──→ AgentEvent::Error (on failure)
//!   ├─ tool_execution::execute_tools_parallel()   parallel tool dispatch
//!   │      ├─ AgentEvent::ToolCallStart           per-tool
//!   │      └─ AgentEvent::ToolCallComplete        per-tool
//!   └─ AgentEvent::IterationComplete              per-iteration
//! ```
//!
//! When a final answer is reached: `AgentEvent::FinalAnswer` → `Ok(content)`.
//! On any guard or limit failure: `AgentEvent::Error` is emitted first, then
//! `Err(AgentError::…)` is returned — the SSE client always sees the reason
//! before the stream closes.

use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{AgentError, AgentLoopPort, LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AgentConfig, AgentEvent, AgentMessage};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::context_pruning::prune_for_budget;
use crate::loop_detection::LoopDetector;
use crate::stagnation::StagnationDetector;
use crate::stream_collector::collect_stream;
use crate::tool_execution::execute_tools_parallel;

// =============================================================================
// Private helpers
// =============================================================================

/// Send an [`AgentEvent::Error`] on `tx`, ignoring send failures.
///
/// Called before every early-return that carries an [`AgentError`], so that
/// SSE consumers always receive an `error` event before the stream closes.
async fn emit_error_event(tx: &mpsc::Sender<AgentEvent>, message: &str) {
    let _ = tx
        .send(AgentEvent::Error {
            message: message.to_owned(),
        })
        .await;
}

// =============================================================================
// Public struct
// =============================================================================

/// Core agentic loop implementation.
///
/// Construct once (cheaply) and call [`AgentLoopPort::run`] for each
/// independent conversation.  The struct itself is stateless; all per-run
/// state lives on the stack inside `run`.
///
/// # Wiring
///
/// ```rust,ignore
/// let agent: Arc<dyn AgentLoopPort> = Arc::new(AgentLoop::new(
///     Arc::new(my_llm_adapter),    // impl LlmCompletionPort
///     Arc::new(my_tool_executor),  // impl ToolExecutorPort
/// ));
/// ```
pub struct AgentLoop {
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
}

impl AgentLoop {
    /// Create a new `AgentLoop` with the provided port implementations.
    pub fn new(llm: Arc<dyn LlmCompletionPort>, tool_executor: Arc<dyn ToolExecutorPort>) -> Self {
        Self { llm, tool_executor }
    }
}

// =============================================================================
// AgentLoopPort implementation
// =============================================================================

#[async_trait]
impl AgentLoopPort for AgentLoop {
    /// Drive the LLM→tool→LLM cycle until a final answer or a termination
    /// condition.
    ///
    /// # Returns
    ///
    /// - `Ok(final_answer)` — the model produced a response without requesting
    ///   further tool calls.
    /// - `Err(AgentError::MaxIterationsReached)` — reached `config.max_iterations`
    ///   without a final answer.
    /// - `Err(AgentError::LoopDetected)` — the same tool batch repeated more
    ///   than `config.max_protocol_strikes` times.
    /// - `Err(AgentError::Internal)` — text stagnation or an infrastructure
    ///   error (LLM stream failure, etc.).
    async fn run(
        &self,
        messages: Vec<AgentMessage>,
        config: AgentConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<String, AgentError> {
        let mut messages = messages;
        let mut loop_detector = LoopDetector::new();
        let mut stagnation_detector = StagnationDetector::new();

        // Discover tools once before the iteration loop — the tool set does not
        // change during a single conversation, and calling list_tools() per
        // iteration would add pointless overhead (and round-trips for MCP).
        let tools = self.tool_executor.list_tools().await;
        debug!(tool_count = tools.len(), "tools available");

        for iteration in 0..config.max_iterations {
            debug!(iteration, "agent loop iteration starting");

            // ---- 1. Context budget pruning ----------------------------------
            messages = prune_for_budget(messages, &config);

            // ---- 2. LLM call (streaming) ------------------------------------
            let stream = match self.llm.chat_stream(&messages, &tools).await {
                Ok(s) => s,
                Err(e) => {
                    let msg = format!("LLM stream error: {e}");
                    emit_error_event(&tx, &msg).await;
                    return Err(AgentError::Internal(msg));
                }
            };

            // ---- 3. Collect stream, forwarding text live --------------------
            let response = match collect_stream(stream, &tx, config.max_parallel_tools).await {
                Ok(r) => r,
                Err(e) => {
                    let msg = format!("stream collection error: {e}");
                    emit_error_event(&tx, &msg).await;
                    return Err(AgentError::Internal(msg));
                }
            };

            debug!(
                content_len = response.content.len(),
                tool_call_count = response.tool_calls.len(),
                finish_reason = %response.finish_reason,
                "LLM response received"
            );

            // ---- 4. Stagnation guard ----------------------------------------
            // `record` is a no-op on empty text (tool-call-only responses);
            // that guard lives inside StagnationDetector to keep the invariant
            // with the module that owns it.
            if let Err(e) =
                stagnation_detector.record(&response.content, config.max_stagnation_steps)
            {
                emit_error_event(&tx, &e.to_string()).await;
                return Err(e);
            }

            // ---- 5. No tool calls → final answer ----------------------------
            if response.tool_calls.is_empty() {
                debug!("no tool calls; final answer reached");
                let _ = tx
                    .send(AgentEvent::FinalAnswer {
                        content: response.content.clone(),
                    })
                    .await;
                return Ok(response.content);
            }

            // ---- 6. Loop detection ------------------------------------------
            if let Err(e) = loop_detector.check(&response.tool_calls, config.max_protocol_strikes) {
                emit_error_event(&tx, &e.to_string()).await;
                return Err(e);
            }

            // ---- 7. Parallel tool execution ---------------------------------
            let results =
                execute_tools_parallel(&response.tool_calls, &self.tool_executor, &config, &tx)
                    .await;

            // ---- 8. Append assistant + tool-result messages -----------------
            messages.push(AgentMessage::Assistant {
                content: if response.content.is_empty() {
                    None
                } else {
                    Some(response.content)
                },
                // Move tool_calls — steps 6 and 7 only borrow &response.tool_calls,
                // so by this point we hold the only reference and no clone is needed.
                tool_calls: Some(response.tool_calls),
            });
            for result in &results {
                messages.push(AgentMessage::Tool {
                    tool_call_id: result.tool_call_id.clone(),
                    content: result.content.clone(),
                });
            }

            // ---- 9. Emit iteration-complete event ---------------------------
            let _ = tx
                .send(AgentEvent::IterationComplete {
                    iteration: iteration + 1,
                    tool_calls: results.len(),
                })
                .await;

            debug!(
                iteration,
                tool_results = results.len(),
                "iteration complete"
            );
        }

        // Max iterations reached
        warn!(max = config.max_iterations, "agent loop hit max iterations");
        let error = AgentError::MaxIterationsReached(config.max_iterations);
        emit_error_event(&tx, &error.to_string()).await;
        Err(error)
    }
}

// Tests live in tests/unit_agent_loop.rs so they can share the richer mock
// infrastructure in tests/common/ with the integration test suite.
