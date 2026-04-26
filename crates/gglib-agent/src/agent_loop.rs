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
    AgentError, AgentLoopPort, AgentRunOutput, EmptyToolExecutor, FilteredToolExecutor,
    LlmCompletionPort, ToolExecutorPort,
};
use gglib_core::{
    AgentConfig, AgentEvent, AgentMessage, AssistantContent, ToolCall, ToolDefinition, ToolResult,
};

use crate::stream_collector::CollectedResponse;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::context_pruning::prune_for_budget;
use crate::loop_detection::LoopDetector;
use crate::stagnation::StagnationDetector;
use crate::stream_collector::collect_stream;
use crate::tool_execution::execute_tools_parallel;
use crate::util::emit_error_event;

// =============================================================================
// Private helpers
// =============================================================================

/// Emit an [`AgentEvent::Error`] and return <code>Err([`AgentError::Internal`])</code>.
///
/// Collapses the repeated pattern:
/// ```text
/// emit_error_event(tx, &msg).await;
/// return Err(AgentError::Internal(msg));
/// ```
/// into:
/// ```text
/// return fail_loop(tx, msg).await;
/// ```
async fn fail_loop<T>(tx: &mpsc::Sender<AgentEvent>, msg: String) -> Result<T, AgentError> {
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
            Err(e) => return fail_loop(tx, format!("LLM stream error: {e}")).await,
        };
        match collect_stream(stream, tx).await {
            Ok(r) => Ok(r),
            Err(e) => fail_loop(tx, format!("stream collection error: {e}")).await,
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

    /// Emit a `FinalAnswer` event, append the assistant reply to `messages`,
    /// and return `Ok(AgentRunOutput)`.
    async fn finalize_answer(
        messages: &mut Vec<AgentMessage>,
        content: String,
        iteration: usize,
        tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<AgentRunOutput, AgentError> {
        debug!("no tool calls; final answer reached");
        let _ = tx
            .send(AgentEvent::FinalAnswer {
                content: content.clone(),
            })
            .await;
        messages.push(AgentMessage::Assistant {
            content: AssistantContent {
                text: Some(content.clone()),
                tool_calls: vec![],
            },
        });
        Ok(AgentRunOutput {
            answer: content,
            history: std::mem::take(messages),
            total_iterations: iteration + 1,
        })
    }

    /// Execute tools, append assistant + tool-result messages, prune the
    /// context budget, and emit `IterationComplete`.
    async fn execute_tool_iteration(
        &self,
        messages: &mut Vec<AgentMessage>,
        response: CollectedResponse,
        config: &AgentConfig,
        iteration: usize,
        tx: &mpsc::Sender<AgentEvent>,
        tools: &[ToolDefinition],
    ) {
        let results =
            execute_tools_parallel(&response.tool_calls, &self.tool_executor, config, tx, tools)
                .await;

        let tool_call_count = results.len();
        let tool_failures = results.iter().filter(|r| !r.success).count();
        append_iteration_messages(messages, response.content, response.tool_calls, results);

        *messages = prune_for_budget(std::mem::take(messages), config);

        let _ = tx
            .send(AgentEvent::IterationComplete {
                iteration: iteration + 1,
                tool_calls: tool_call_count,
            })
            .await;

        debug!(
            iteration,
            tool_results = tool_call_count,
            tool_failures,
            "iteration complete"
        );
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
    /// - `Ok(AgentRunOutput)` — the model produced a response without
    ///   requesting further tool calls.
    /// - `Err(AgentError::MaxIterationsReached)` — reached `config.max_iterations`
    ///   without a final answer.
    /// - `Err(AgentError::LoopDetected)` — the same tool batch repeated more
    ///   than `config.max_repeated_batch_steps` times.
    /// - `Err(AgentError::StagnationDetected)` — the assistant repeated the same
    ///   text content for too many consecutive iterations.
    async fn run(
        &self,
        mut messages: Vec<AgentMessage>,
        config: AgentConfig,
        tx: mpsc::Sender<AgentEvent>,
    ) -> Result<AgentRunOutput, AgentError> {
        // Validate unconditionally — the cost is four integer comparisons.
        // Invalid configs are a caller bug and must never silently proceed.
        // `validated()` consumes and returns `config`, avoiding a redundant clone.
        let config = config
            .validated()
            .map_err(|e| AgentError::Internal(format!("AgentConfig invariants violated: {e}")))?;

        let mut guards = Guards::default();

        let tools = self.tool_executor.list_tools().await;
        debug!(tool_count = tools.len(), "tools available");

        messages = prune_for_budget(messages, &config);

        for iteration in 0..config.max_iterations {
            debug!(iteration, "agent loop iteration starting");

            let response = self.call_and_collect(&messages, &tools, &tx).await?;

            if response.tool_calls.len() > config.max_parallel_tools {
                // Soft recovery (was: hard `Err(ParallelToolLimitExceeded)`).
                //
                // Modern reasoning models occasionally request very large
                // parallel batches; aborting the entire turn made the council
                // appear to "stall" with no visible cause.  Instead we:
                //
                // 1. Emit a visible `SystemWarning` to the SSE stream so the
                //    user sees what happened and gets an actionable hint.
                // 2. Synthesise tool-result messages that report the overflow
                //    back to the model as a tool error, so it can self-correct
                //    by retrying with a smaller batch.
                // 3. Append assistant + synthetic results to history and
                //    continue the loop.
                let count = response.tool_calls.len();
                let limit = config.max_parallel_tools;

                let synthetic_error = format!(
                    "ERROR: You requested {count} tool calls in a single batch, but the \
                     parallel tool-call limit is {limit}.  None of the {count} calls were \
                     executed.  Please retry your request, issuing at most {limit} tool \
                     calls per turn (you can split a large batch across multiple turns)."
                );
                let synthetic_results: Vec<ToolResult> = response
                    .tool_calls
                    .iter()
                    .map(|tc| ToolResult {
                        tool_call_id: tc.id.clone(),
                        content: synthetic_error.clone(),
                        success: false,
                    })
                    .collect();

                let suggested_action = format!(
                    "To permanently raise this limit, run: \
                     `gglib config settings set --max-parallel-tools <N>` \
                     (current: {limit}, ceiling: {ceiling})",
                    ceiling = gglib_core::MAX_PARALLEL_TOOLS_CEILING,
                );
                let warning_message = format!(
                    "Agent attempted {count} parallel tool calls (limit is {limit}). \
                     Auto-recovering: the model will retry in smaller batches."
                );
                warn!(count, limit, "parallel tool limit exceeded; soft-recovering");
                let _ = tx
                    .send(AgentEvent::SystemWarning {
                        message: warning_message,
                        suggested_action: Some(suggested_action),
                    })
                    .await;

                append_iteration_messages(
                    &mut messages,
                    response.content,
                    response.tool_calls,
                    synthetic_results,
                );
                messages = prune_for_budget(std::mem::take(&mut messages), &config);

                let _ = tx
                    .send(AgentEvent::IterationComplete {
                        iteration: iteration + 1,
                        tool_calls: count,
                    })
                    .await;
                continue;
            }

            debug!(
                content_len = response.content.len(),
                reasoning_len = response.reasoning_content.len(),
                tool_call_count = response.tool_calls.len(),
                finish_reason = %response.finish_reason,
                "LLM response received"
            );

            // Guards run BEFORE the finalize check so that:
            // - Stagnation catches repeated text on both tool-calling and
            //   text-only iterations.
            // - Loop detection catches repeated tool-call batches (skipped
            //   when tool_calls is empty to avoid a degenerate signature).
            guards
                .check(&config, &response.content, &response.tool_calls, &tx)
                .await?;

            if response.tool_calls.is_empty() {
                return Self::finalize_answer(&mut messages, response.content, iteration, &tx)
                    .await;
            }

            self.execute_tool_iteration(&mut messages, response, &config, iteration, &tx, &tools)
                .await;
        }

        warn!(max = config.max_iterations, "agent loop hit max iterations");
        let error = AgentError::MaxIterationsReached(config.max_iterations);
        emit_error_event(&tx, &error.to_string()).await;
        Err(error)
    }
}

/// Bundles the stagnation and loop-detection detectors so they can be passed
/// as a single unit rather than two independent `&mut` parameters.
///
/// Guards whose corresponding `Option` field in [`AgentConfig`] is `None` are
/// skipped entirely — `None` disables the guard (e.g. in tests that reuse a
/// fixed LLM response or deliberately repeat the same tool call batch).
#[derive(Default)]
struct Guards {
    stagnation: StagnationDetector,
    loop_detector: LoopDetector,
}

impl Guards {
    /// Check both stagnation and loop-detection guards against the current
    /// iteration's response.
    ///
    /// Stagnation is checked on **every** iteration — including ones that will
    /// become the final answer — so the detector catches models that repeat
    /// the same text regardless of whether tool calls are present.
    ///
    /// Loop detection is only checked when tool calls are present, since an
    /// empty batch would produce a degenerate signature.
    ///
    /// On failure, emits an [`AgentEvent::Error`] on `tx` before returning so
    /// SSE consumers always see the failure reason before the stream closes.
    async fn check(
        &mut self,
        config: &AgentConfig,
        content: &str,
        tool_calls: &[ToolCall],
        tx: &mpsc::Sender<AgentEvent>,
    ) -> Result<(), AgentError> {
        if let Some(max_steps) = config.max_stagnation_steps {
            if let Err(e) = self.stagnation.record(content, max_steps) {
                emit_error_event(tx, &e.to_string()).await;
                return Err(e);
            }
        }
        if !tool_calls.is_empty() {
            if let Some(max_steps) = config.max_repeated_batch_steps {
                if let Err(e) = self.loop_detector.check(tool_calls, max_steps) {
                    emit_error_event(tx, &e.to_string()).await;
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

/// Append an assistant turn and its tool results to `messages`.
///
/// Selects the correct [`AssistantContent`] variant based on whether
/// `content` is empty, avoiding the vacuous all-`None` state.
fn append_iteration_messages(
    messages: &mut Vec<AgentMessage>,
    content: String,
    tool_calls: Vec<ToolCall>,
    results: Vec<ToolResult>,
) {
    let assistant = AgentMessage::Assistant {
        content: AssistantContent {
            text: if content.is_empty() {
                None
            } else {
                Some(content)
            },
            tool_calls,
        },
    };
    messages.push(assistant);
    for result in results {
        messages.push(AgentMessage::Tool {
            tool_call_id: result.tool_call_id,
            content: result.content,
        });
    }
}

// Tests live in tests/unit_agent_loop.rs so they can share the richer mock
// infrastructure in tests/common/ with the integration test suite.
