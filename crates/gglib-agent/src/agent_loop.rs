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

        for iteration in 0..config.max_iterations {
            debug!(iteration, "agent loop iteration starting");

            // ---- 1. Context budget pruning ----------------------------------
            messages = prune_for_budget(messages, &config);

            // ---- 2. Tool discovery ------------------------------------------
            let tools = self.tool_executor.list_tools().await;
            debug!(tool_count = tools.len(), "tools available");

            // ---- 3. LLM call (streaming) ------------------------------------
            let stream = self
                .llm
                .chat_stream(&messages, &tools)
                .await
                .map_err(|e| AgentError::Internal(format!("LLM stream error: {e}")))?;

            // ---- 4. Collect stream, forwarding text live --------------------
            let response = collect_stream(stream, &tx)
                .await
                .map_err(|e| AgentError::Internal(format!("stream collection error: {e}")))?;

            debug!(
                content_len = response.content.len(),
                tool_call_count = response.tool_calls.len(),
                finish_reason = %response.finish_reason,
                "LLM response received"
            );

            // ---- 5. Stagnation guard ----------------------------------------
            if let Err(e) =
                stagnation_detector.record(&response.content, config.max_stagnation_steps)
            {
                emit_error_event(&tx, &e.to_string()).await;
                return Err(e);
            }

            // ---- 6. No tool calls → final answer ----------------------------
            if response.tool_calls.is_empty() {
                debug!("no tool calls; final answer reached");
                let _ = tx
                    .send(AgentEvent::FinalAnswer {
                        content: response.content.clone(),
                    })
                    .await;
                return Ok(response.content);
            }

            // ---- 7. Loop detection ------------------------------------------
            if let Err(e) = loop_detector.check(&response.tool_calls, config.max_protocol_strikes) {
                emit_error_event(&tx, &e.to_string()).await;
                return Err(e);
            }

            // ---- 8. Parallel tool execution ---------------------------------
            let results =
                execute_tools_parallel(&response.tool_calls, &self.tool_executor, &config, &tx)
                    .await;

            // ---- 9. Append assistant + tool-result messages -----------------
            messages.push(AgentMessage::Assistant {
                content: if response.content.is_empty() {
                    None
                } else {
                    Some(response.content)
                },
                tool_calls: Some(response.tool_calls.clone()),
            });
            for result in &results {
                messages.push(AgentMessage::Tool {
                    tool_call_id: result.tool_call_id.clone(),
                    content: result.content.clone(),
                });
            }

            // ---- 10. Emit iteration-complete event --------------------------
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
        let message = format!(
            "Agent loop reached the maximum number of iterations ({})",
            config.max_iterations
        );
        let _ = tx
            .send(AgentEvent::Error {
                message: message.clone(),
            })
            .await;
        Err(AgentError::MaxIterationsReached(config.max_iterations))
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::sync::Arc;

    use async_trait::async_trait;
    use futures_util::stream;
    use gglib_core::ports::{AgentError, AgentLoopPort, LlmCompletionPort, ToolExecutorPort};
    use gglib_core::{
        AgentConfig, AgentMessage, LlmStreamEvent, ToolCall, ToolDefinition, ToolResult,
    };
    use tokio::sync::{Mutex, mpsc};

    use super::*;

    // ---- Mock LLM -----------------------------------------------------------

    struct MockLlm {
        /// Pre-configured responses popped in order on each `chat_stream` call.
        responses: Mutex<std::collections::VecDeque<Vec<LlmStreamEvent>>>,
    }

    impl MockLlm {
        fn new(responses: Vec<Vec<LlmStreamEvent>>) -> Self {
            Self {
                responses: Mutex::new(responses.into_iter().collect()),
            }
        }
    }

    #[async_trait]
    impl LlmCompletionPort for MockLlm {
        async fn chat_stream(
            &self,
            _messages: &[AgentMessage],
            _tools: &[ToolDefinition],
        ) -> anyhow::Result<
            Pin<Box<dyn futures_core::Stream<Item = anyhow::Result<LlmStreamEvent>> + Send>>,
        > {
            let events = self
                .responses
                .lock()
                .await
                .pop_front()
                .ok_or_else(|| anyhow::anyhow!("mock LLM has no more responses"))?;
            Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
        }
    }

    // ---- Mock tool executor -------------------------------------------------

    struct MockExecutor;

    #[async_trait]
    impl ToolExecutorPort for MockExecutor {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![ToolDefinition::new("do_thing")]
        }
        async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content: "done".into(),
                success: true,
                wait_ms: 0,
                duration_ms: 0,
            })
        }
    }

    // ---- Helpers ------------------------------------------------------------

    fn tool_call_response(id: &str, name: &str) -> Vec<LlmStreamEvent> {
        vec![
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some(id.into()),
                name: Some(name.into()),
                arguments: Some("{}".into()),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ]
    }

    fn text_response(text: &str) -> Vec<LlmStreamEvent> {
        vec![
            LlmStreamEvent::TextDelta {
                content: text.into(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ]
    }

    fn text_and_tool_call_response(text: &str, id: &str, name: &str) -> Vec<LlmStreamEvent> {
        vec![
            LlmStreamEvent::TextDelta {
                content: text.into(),
            },
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some(id.into()),
                name: Some(name.into()),
                arguments: Some("{}".into()),
            },
            LlmStreamEvent::Done {
                finish_reason: "tool_calls".into(),
            },
        ]
    }

    // ---- Tests --------------------------------------------------------------

    #[tokio::test]
    async fn two_iteration_loop_produces_final_answer() {
        let llm = Arc::new(MockLlm::new(vec![
            // Iteration 1: request a tool call
            tool_call_response("c1", "do_thing"),
            // Iteration 2: return final answer
            text_response("The answer is 42."),
        ]));
        let agent = AgentLoop::new(llm, Arc::new(MockExecutor));
        let (tx, _rx) = mpsc::channel(64);

        let result = agent
            .run(
                vec![AgentMessage::User {
                    content: "what is the answer?".into(),
                }],
                AgentConfig::default(),
                tx,
            )
            .await;

        assert_eq!(result.unwrap(), "The answer is 42.");
    }

    #[tokio::test]
    async fn max_iterations_exceeded_returns_error() {
        // Feed only tool-call responses so the loop never finishes naturally.
        let responses: Vec<Vec<LlmStreamEvent>> = (0..30)
            .map(|i| tool_call_response(&format!("c{i}"), "do_thing"))
            .collect();

        let llm = Arc::new(MockLlm::new(responses));
        let agent = AgentLoop::new(llm, Arc::new(MockExecutor));
        let (tx, _rx) = mpsc::channel(64);

        let config = AgentConfig {
            max_iterations: 3,
            // Disable loop detection and stagnation so only max_iterations fires
            max_protocol_strikes: 100,
            max_stagnation_steps: 100,
            ..Default::default()
        };

        let err = agent
            .run(
                vec![AgentMessage::User {
                    content: "go".into(),
                }],
                config,
                tx,
            )
            .await
            .unwrap_err();

        assert!(matches!(err, AgentError::MaxIterationsReached(3)));
    }

    #[tokio::test]
    async fn loop_detection_fires_on_repeated_tool_batch() {
        // Repeat the exact same tool call 4 times.
        let responses: Vec<Vec<LlmStreamEvent>> = (0..10)
            .map(|_| tool_call_response("c1", "do_thing")) // identical each time
            .collect();

        let llm = Arc::new(MockLlm::new(responses));
        let agent = AgentLoop::new(llm, Arc::new(MockExecutor));
        let (tx, mut rx) = mpsc::channel(64);

        let config = AgentConfig {
            max_iterations: 10,
            max_protocol_strikes: 2,
            max_stagnation_steps: 100,
            ..Default::default()
        };

        let err = agent
            .run(
                vec![AgentMessage::User {
                    content: "go".into(),
                }],
                config,
                tx,
            )
            .await
            .unwrap_err();

        assert!(
            matches!(err, AgentError::LoopDetected { .. }),
            "expected LoopDetected, got {err:?}"
        );

        // An AgentEvent::Error must be emitted before the stream closes.
        let mut got_error_event = false;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, AgentEvent::Error { .. }) {
                got_error_event = true;
            }
        }
        assert!(
            got_error_event,
            "expected AgentEvent::Error to be emitted for LoopDetected"
        );
    }

    #[tokio::test]
    async fn stagnation_detected_on_repeated_text() {
        // Stagnation fires when the model produces the same text across consecutive
        // iterations where the loop *continues* (i.e. tool calls are also present).
        // A response with no tool calls causes FinalAnswer on the first occurrence
        // before stagnation can accumulate — so we must pair text with a tool call.
        //
        // Setup: LLM always emits "Thinking…" text + a tool call (unique ID each
        // time to prevent loop detection from firing first).  Stagnation accumulates
        // until max_stagnation_steps is reached.
        let responses: Vec<Vec<LlmStreamEvent>> = (0..10)
            .map(|i| text_and_tool_call_response("Thinking...", &format!("c{i}"), "do_thing"))
            .collect();

        let llm = Arc::new(MockLlm::new(responses));
        let agent = AgentLoop::new(llm, Arc::new(MockExecutor));
        let (tx, mut rx) = mpsc::channel(64);

        let config = AgentConfig {
            max_iterations: 10,
            max_protocol_strikes: 100, // disable loop detection
            max_stagnation_steps: 2,   // fires on the 3rd identical-text iteration
            ..Default::default()
        };

        let err = agent
            .run(
                vec![AgentMessage::User {
                    content: "go".into(),
                }],
                config,
                tx,
            )
            .await
            .unwrap_err();

        assert!(
            matches!(err, AgentError::Internal(_)),
            "expected Internal (stagnation), got {err:?}"
        );

        // An AgentEvent::Error must be emitted before the stream closes.
        let mut got_error_event = false;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, AgentEvent::Error { .. }) {
                got_error_event = true;
            }
        }
        assert!(
            got_error_event,
            "expected AgentEvent::Error to be emitted for stagnation abort"
        );
    }

    #[tokio::test]
    async fn iteration_complete_events_are_emitted() {
        let llm = Arc::new(MockLlm::new(vec![
            tool_call_response("c1", "do_thing"),
            text_response("done"),
        ]));
        let agent = AgentLoop::new(llm, Arc::new(MockExecutor));
        let (tx, mut rx) = mpsc::channel(64);

        agent
            .run(
                vec![AgentMessage::User {
                    content: "go".into(),
                }],
                AgentConfig::default(),
                tx,
            )
            .await
            .unwrap();

        let mut got_iteration_complete = false;
        let mut got_final_answer = false;
        while let Ok(evt) = rx.try_recv() {
            match evt {
                AgentEvent::IterationComplete { iteration: 1, .. } => got_iteration_complete = true,
                AgentEvent::FinalAnswer { .. } => got_final_answer = true,
                _ => {}
            }
        }
        assert!(
            got_iteration_complete,
            "IterationComplete should be emitted after iteration 1"
        );
        assert!(got_final_answer, "FinalAnswer should be emitted");
    }
}
