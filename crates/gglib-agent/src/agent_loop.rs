//! [`AgentLoopPort`] implementation: the main LLM→tool→LLM state machine.
//!
//! This module wires all the utilities together:
//!
//! ```text
//! AgentLoop::run()
//!   │
//!   ├─ context_pruning::prune_for_budget()        initial budget trim (before loop)
//!   │
//!   └─ [per iteration]
//!       ├─ llm.chat_stream()                          LLM call (streaming)
//!       ├─ stream_collector::collect_stream()          text forwarded live ──→ AgentEvent::TextDelta
//!       ├─ stagnation::StagnationDetector::record()   stagnation guard ──→ AgentEvent::Error (on failure)
//!       ├─ loop_detection::LoopDetector::check()      loop guard       ──→ AgentEvent::Error (on failure)
//!       ├─ tool_execution::execute_tools_parallel()   parallel tool dispatch
//!       │      ├─ AgentEvent::ToolCallStart           per-tool
//!       │      └─ AgentEvent::ToolCallComplete        per-tool
//!       ├─ context_pruning::prune_for_budget()        post-append budget trim
//!       └─ AgentEvent::IterationComplete              per-iteration
//! ```
//!
//! When a final answer is reached: `AgentEvent::FinalAnswer` → `Ok(content)`.
//! On any guard or limit failure: `AgentEvent::Error` is emitted first, then
//! `Err(AgentError::…)` is returned — the SSE client always sees the reason
//! before the stream closes.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::{
    AgentError, AgentLoopPort, AgentRunOutput, LlmCompletionPort, ToolExecutorPort,
};
use gglib_core::{
    AgentConfig, AgentEvent, AgentMessage, AssistantContent, ToolDefinition, ToolResult,
};

use crate::stream_collector::CollectedResponse;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::context_pruning::{prune_for_budget, total_chars};
use crate::filter::{EmptyToolExecutor, FilteredToolExecutor};
use crate::loop_detection::LoopDetector;
use crate::stagnation::StagnationDetector;
use crate::stream_collector::collect_stream;
use crate::tool_execution::execute_tools_parallel;

// =============================================================================
// Private helpers
// =============================================================================

/// Emit an [`AgentEvent::Error`] on `tx`, ignoring send failures.
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

/// Emit an [`AgentEvent::Error`] and return `Err(`[`AgentError::Internal`]`)`.
///
/// Collapses the repeated pattern:
/// ```text
/// emit_error_event(tx, &msg).await;
/// return Err(AgentError::Internal(msg));
/// ```
/// into:
/// ```text
/// return bail_internal(tx, msg).await;
/// ```
async fn bail_internal<T>(tx: &mpsc::Sender<AgentEvent>, msg: String) -> Result<T, AgentError> {
    emit_error_event(tx, &msg).await;
    Err(AgentError::Internal(msg))
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
/// Prefer [`AgentLoop::build`] at composition roots:
///
/// ```rust,ignore
/// let agent: Arc<dyn AgentLoopPort> = AgentLoop::build(
///     Arc::new(my_llm_adapter),    // impl LlmCompletionPort
///     Arc::new(my_tool_executor),  // impl ToolExecutorPort
///     None,                        // no tool filter
/// );
/// ```
pub struct AgentLoop {
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
}

impl AgentLoop {
    /// Create a new `AgentLoop` with the provided LLM and tool-executor ports.
    ///
    /// The `tool_executor` is used as-is — no filter is applied.  Use
    /// [`AgentLoop::build`] at composition roots; it handles the tool-filter
    /// contract (`Some([])` → zero tools, `None` → all tools) and returns the
    /// type-erased `Arc<dyn AgentLoopPort>`.
    ///
    /// `new` is intentionally crate-private so that external callers cannot
    /// bypass the filter contract and accidentally expose all tools when the
    /// intent was an empty allowlist.
    pub(crate) fn new(
        llm: Arc<dyn LlmCompletionPort>,
        tool_executor: Arc<dyn ToolExecutorPort>,
    ) -> Self {
        Self { llm, tool_executor }
    }

    /// Call the LLM (step 2) then collect the stream (step 3) into a
    /// [`CollectedResponse`].
    ///
    /// Both the LLM call and stream-collection errors are translated into
    /// [`AgentError::Internal`] after emitting an [`AgentEvent::Error`] on `tx`
    /// so that SSE consumers always see the failure reason before the stream
    /// closes.
    async fn call_and_collect(
        &self,
        messages: &[AgentMessage],
        tools: &[ToolDefinition],
        tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<CollectedResponse, AgentError> {
        let stream = match self.llm.chat_stream(messages, tools).await {
            Ok(s) => s,
            Err(e) => return bail_internal(tx, format!("LLM stream error: {e}")).await,
        };
        match collect_stream(stream, tx).await {
            Ok(r) => Ok(r),
            Err(e) => bail_internal(tx, format!("stream collection error: {e}")).await,
        }
    }

    /// Compose an `AgentLoop` as `Arc<dyn AgentLoopPort>`, optionally wrapping
    /// `tool_executor` in a [`FilteredToolExecutor`] allowlist.
    ///
    /// This is the preferred construction path for both HTTP handlers and CLI
    /// callers, eliminating the boilerplate `Arc::new` + optional filter-wrapping
    /// that would otherwise be duplicated at every call site.
    ///
    /// # Parameters
    ///
    /// * `tool_filter` — `Some(set)` restricts the visible and executable tools
    ///   to the names in `set`; `None` exposes all tools from `tool_executor`.
    pub fn build(
        llm: Arc<dyn LlmCompletionPort>,
        tool_executor: Arc<dyn ToolExecutorPort>,
        tool_filter: Option<HashSet<String>>,
    ) -> Arc<dyn AgentLoopPort> {
        let executor: Arc<dyn ToolExecutorPort> = match tool_filter {
            // No filter supplied — expose every tool from the inner executor.
            None => tool_executor,
            // Empty allowlist — the caller explicitly wants zero tools exposed.
            // Fall through to EmptyToolExecutor rather than exposing all tools;
            // `Some([])` must never be silently interpreted as "no restriction".
            Some(allowed) if allowed.is_empty() => Arc::new(EmptyToolExecutor),
            // Non-empty allowlist — restrict to the named set.
            Some(allowed) => Arc::new(FilteredToolExecutor::new(tool_executor, allowed)),
        };
        Arc::new(Self::new(llm, executor))
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
    /// - `Ok((final_answer, messages))` — the model produced a response without
    ///   requesting further tool calls. `messages` is the full conversation
    ///   history including every assistant and tool-result message appended
    ///   during the loop, plus the final assistant reply.
    /// - `Err(AgentError::MaxIterationsReached)` — reached `config.max_iterations`
    ///   without a final answer.
    /// - `Err(AgentError::LoopDetected)` — the same tool batch repeated more
    ///   than `config.max_repeated_batch_steps` times.
    ///   - `Err(AgentError::StagnationDetected)` — the assistant repeated the same
    ///   text content for too many consecutive iterations.
    async fn run(
        &self,
        messages: Vec<AgentMessage>,
        config: AgentConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<AgentRunOutput, AgentError> {
        let mut messages = messages;
        let mut loop_detector = LoopDetector::default();
        let mut stagnation_detector = StagnationDetector::default();

        // Discover tools once before the iteration loop — the tool set does not
        // change during a single conversation, and calling list_tools() per
        // iteration would add pointless overhead (and round-trips for MCP).
        let tools = self.tool_executor.list_tools().await;
        debug!(tool_count = tools.len(), "tools available");

        // Track the total character count incrementally so that
        // `prune_for_budget` never has to re-scan the entire history.
        // Updated after every prune (inside `prune_for_budget`) and after
        // every `push_iteration_messages_returning_char_delta` call (via the returned delta).
        let mut running_chars = total_chars(&messages);

        // Prune the caller-supplied history once before the first LLM call so
        // that oversized initial contexts are handled even when the model
        // returns a final answer on the very first iteration (no append step).
        messages = prune_for_budget(messages, &config, &mut running_chars);

        for iteration in 0..config.max_iterations {
            debug!(iteration, "agent loop iteration starting");

            // ---- 1+2. LLM call and stream collection ----------------------
            let response = self.call_and_collect(&messages, &tools, &tx).await?;

            // Guard: reject tool-call batches that exceed the configured
            // concurrency cap.  (The stream collector applies a hard index cap
            // during parsing; this check enforces the user-visible limit.)
            //
            // This is a model protocol violation, not an internal infrastructure
            // failure, so it returns ParallelToolLimitExceeded rather than Internal —
            // callers can distinguish and report it differently.
            if response.tool_calls.len() > config.max_parallel_tools {
                let error = AgentError::ParallelToolLimitExceeded {
                    count: response.tool_calls.len(),
                    limit: config.max_parallel_tools,
                };
                emit_error_event(&tx, &error.to_string()).await;
                return Err(error);
            }

            debug!(
                content_len = response.content.len(),
                reasoning_len = response.reasoning_content.len(),
                tool_call_count = response.tool_calls.len(),
                finish_reason = %response.finish_reason,
                "LLM response received"
            );

            // ---- 3. No tool calls → final answer ----------------------------
            // Checked BEFORE the stagnation guard: a model that says "I’m
            // done" in the same wording as a prior non-final turn must not be
            // penalised as stagnating — the absence of tool calls is the
            // definitive signal that the loop completed normally.
            if response.tool_calls.is_empty() {
                debug!("no tool calls; final answer reached");
                let content = response.content;
                let _ = tx
                    .send(AgentEvent::FinalAnswer {
                        content: content.clone(),
                    })
                    .await;
                // Append the final assistant reply so callers receive the
                // complete accumulated history and can pass it back unchanged
                // as the `messages` argument for the next REPL turn.
                messages.push(AgentMessage::Assistant {
                    content: AssistantContent::Content(content.clone()),
                });
                return Ok(AgentRunOutput {
                    answer: content,
                    history: if config.return_history {
                        Some(messages)
                    } else {
                        None
                    },
                    total_iterations: iteration + 1,
                });
            }

            // ---- 4. Stagnation guard ----------------------------------------
            // Only evaluated on turns that produced tool calls (non-final
            // turns), so a repeated terminal text never fires this guard.
            // `record` is a no-op on empty text; that guard lives inside
            // StagnationDetector.  When `max_stagnation_steps` is `None` the
            // guard is disabled (e.g. in tests that reuse a fixed LLM response).
            if let Some(max_steps) = config.max_stagnation_steps {
                if let Err(e) = stagnation_detector.record(&response.content, max_steps) {
                    emit_error_event(&tx, &e.to_string()).await;
                    return Err(e);
                }
            }

            // ---- 5. Loop detection ------------------------------------------
            // When `max_repeated_batch_steps` is `None` the guard is disabled
            // (e.g. in tests that deliberately repeat the same tool call to
            // exercise multi-iteration behaviour without hitting the limit).
            if let Some(max_steps) = config.max_repeated_batch_steps {
                if let Err(e) = loop_detector.check(&response.tool_calls, max_steps) {
                    emit_error_event(&tx, &e.to_string()).await;
                    return Err(e);
                }
            }

            // ---- 6. Parallel tool execution ---------------------------------
            let results =
                execute_tools_parallel(&response.tool_calls, &self.tool_executor, &config, &tx)
                    .await;

            // ---- 7. Append assistant + tool-result messages -----------------
            // Capture len before consuming results so we can report the count
            // in the IterationComplete event without keeping a reference.
            let tool_call_count = results.len();
            running_chars += append_iteration_messages(
                &mut messages,
                response.content,
                response.tool_calls,
                results,
            );

            // ---- 8. Context budget pruning (applied after new messages added) --
            messages = prune_for_budget(messages, &config, &mut running_chars);

            // ---- 9. Emit iteration-complete event ---------------------------
            let _ = tx
                .send(AgentEvent::IterationComplete {
                    iteration: iteration + 1,
                    tool_calls: tool_call_count,
                })
                .await;

            debug!(
                iteration,
                tool_results = tool_call_count,
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

/// Append an assistant turn and its tool results to `messages`, returning
/// the total character delta so the caller can maintain `running_chars`
/// without re-scanning the full history.
///
/// Selects the correct [`AssistantContent`] variant based on whether
/// `content` is empty, avoiding the vacuous all-`None` state.
fn append_iteration_messages(
    messages: &mut Vec<AgentMessage>,
    content: String,
    tool_calls: Vec<gglib_core::ToolCall>,
    results: Vec<ToolResult>,
) -> usize {
    let assistant = AgentMessage::Assistant {
        content: if content.is_empty() {
            AssistantContent::ToolCalls(tool_calls)
        } else {
            AssistantContent::Both(content, tool_calls)
        },
    };
    let mut added = assistant.char_count();
    messages.push(assistant);
    for result in results {
        let msg = AgentMessage::Tool {
            tool_call_id: result.tool_call_id,
            content: result.content,
        };
        added += msg.char_count();
        messages.push(msg);
    }
    added
}

// Tests live in tests/unit_agent_loop.rs so they can share the richer mock
// infrastructure in tests/common/ with the integration test suite.
