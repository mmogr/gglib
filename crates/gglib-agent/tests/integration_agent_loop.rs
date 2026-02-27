//! Integration tests for the backend agentic loop.
//!
//! All tests are fully in-process: they drive [`AgentLoop`] directly through
//! the [`AgentLoopPort`] interface using [`MockLlmPort`] and
//! [`MockToolExecutorPort`] — no HTTP server, no llama-server process, no MCP
//! daemon.  The suite completes in a few seconds.
//!
//! # Coverage
//!
//! | Test | Guard exercised |
//! |------|----------------|
//! | [`test_simple_tool_call_cycle`] | Basic LLM→tool→LLM round-trip |
//! | [`test_parallel_tool_calls`] | Multiple tool calls in one iteration |
//! | [`test_max_iterations_reached`] | [`AgentConfig::max_iterations`] limit |
//! | [`test_tool_timeout`] | Per-tool timeout → `success = false` result |
//! | [`test_loop_detection`] | Repeated tool-call batch → [`AgentError::LoopDetected`] |
//! | [`test_context_budget_pruning`] | Oversized history → pruning → loop continues |

mod common;

use std::sync::Arc;

use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage, ToolCall, ToolDefinition};
use gglib_core::ports::{AgentError, AgentLoopPort};
use serde_json::json;
use tokio::sync::mpsc;

// =============================================================================
// Helpers
// =============================================================================

/// Drain all events that were sent to `rx` before the channel was closed.
///
/// `AgentLoopPort::run` takes the `Sender` by value and drops it on return,
/// so by the time we call this the channel is already closed; we just read
/// what's buffered.
async fn collect_events(mut rx: mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(evt) = rx.recv().await {
        events.push(evt);
    }
    events
}

/// Return `true` when `events` contains at least one [`AgentEvent::FinalAnswer`].
fn has_final_answer(events: &[AgentEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, AgentEvent::FinalAnswer { .. }))
}

/// Return `true` when `events` contains at least one
/// [`AgentEvent::ToolCallStart`] with the given tool name.
fn has_tool_start(events: &[AgentEvent], name: &str) -> bool {
    events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolCallStart { tool_call, .. } if tool_call.name == name))
}

/// Return `true` when `events` contains at least one
/// [`AgentEvent::ToolCallComplete`] whose result has the given `success` value.
fn has_tool_complete_with_success(events: &[AgentEvent], success: bool) -> bool {
    events.iter().any(
        |e| matches!(e, AgentEvent::ToolCallComplete { result, .. } if result.success == success),
    )
}

// =============================================================================
// Tests
// =============================================================================

/// **Simple tool-call cycle**: LLM requests one tool → tool executes → LLM
/// produces the final answer.
///
/// Exercises the core happy path from the first iteration through to
/// `FinalAnswer`.
#[tokio::test]
async fn test_simple_tool_call_cycle() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "tc1",
                "search",
                json!({"q": "rust"}),
            ))
            .push(MockLlmResponse::text("Here are the results.")),
    );

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search").with_description("Full-text search"),
        MockToolBehavior::Immediate {
            content: "result: async programming".into(),
        },
    );
    let log = executor.call_log_handle();

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "Find info about Rust".into(),
            }],
            AgentConfig::default(),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert_eq!(result.unwrap(), "Here are the results.");

    // Tool was called exactly once with the right name.
    let calls = log.snapshot().await;
    assert_eq!(calls.len(), 1, "expected exactly one tool invocation");
    assert_eq!(calls[0].0, "search");

    // Event stream contains the expected milestones.
    assert!(has_tool_start(&events, "search"), "missing ToolCallStart");
    assert!(
        has_tool_complete_with_success(&events, true),
        "missing successful ToolCallComplete"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::IterationComplete { iteration: 1, .. })),
        "missing IterationComplete for iteration 1"
    );
    assert!(has_final_answer(&events), "missing FinalAnswer");
}

/// **Parallel tool calls**: LLM requests three tools in a single batch → all
/// three execute (possibly concurrently) → LLM produces the final answer.
///
/// Verifies that the `execute_tools_parallel` helper dispatches all calls
/// and that the agent loop correctly builds the follow-up conversation.
#[tokio::test]
async fn test_parallel_tool_calls() {
    let batch = MockLlmResponse::tool_calls(vec![
        ToolCall {
            id: "tc1".into(),
            name: "search".into(),
            arguments: json!({"q": "Rust"}),
        },
        ToolCall {
            id: "tc2".into(),
            name: "search".into(),
            arguments: json!({"q": "async"}),
        },
        ToolCall {
            id: "tc3".into(),
            name: "search".into(),
            arguments: json!({"q": "tokio"}),
        },
    ]);

    let llm = Arc::new(
        MockLlmPort::new()
            .push(batch)
            .push(MockLlmResponse::text("All done.")),
    );

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate {
            content: "ok".into(),
        },
    );
    let log = executor.call_log_handle();

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "Search three topics".into(),
            }],
            AgentConfig {
                max_parallel_tools: 3,
                ..AgentConfig::default()
            },
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert_eq!(result.unwrap(), "All done.");

    // All three tool calls were executed.
    let calls = log.snapshot().await;
    assert_eq!(calls.len(), 3, "expected 3 tool invocations");

    // Three ToolCallStart and three ToolCallComplete events.
    let starts = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::ToolCallStart { .. }))
        .count();
    let completes = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::ToolCallComplete { .. }))
        .count();
    assert_eq!(starts, 3, "expected 3 ToolCallStart events");
    assert_eq!(completes, 3, "expected 3 ToolCallComplete events");
    assert!(has_final_answer(&events));
}

/// **Max iterations reached**: the LLM keeps requesting tool calls without
/// ever producing a final answer.  The loop must terminate with
/// [`AgentError::MaxIterationsReached`] after `max_iterations` iterations.
#[tokio::test]
async fn test_max_iterations_reached() {
    // Provide more responses than max_iterations so the LLM never "runs out".
    let llm = Arc::new(MockLlmPort::new().push_many(
        (0..10).map(|i| MockLlmResponse::tool_call(format!("tc{i}"), "do_thing", json!({}))),
    ));

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("do_thing"),
        MockToolBehavior::Immediate {
            content: "done".into(),
        },
    );

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "go".into(),
            }],
            AgentConfig {
                max_iterations: 3,
                max_protocol_strikes: 100, // disable loop detection for this test
                max_stagnation_steps: 100, // disable stagnation for this test
                ..AgentConfig::default()
            },
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // The loop must report the correct error.
    assert!(
        matches!(result, Err(AgentError::MaxIterationsReached(3))),
        "expected MaxIterationsReached(3), got: {result:?}"
    );

    // The event stream should contain an Error event.
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
        "expected at least one Error event in the stream"
    );

    // No FinalAnswer should have been emitted.
    assert!(!has_final_answer(&events), "unexpected FinalAnswer");
}

/// **Tool timeout**: a slow tool exceeds the per-tool deadline.  The loop must
/// continue by injecting a failed `ToolResult` into the conversation so the
/// LLM can observe and react to the timeout.
///
/// Verifies that:
/// - The timed-out tool produces a `ToolCallComplete` with `success = false`.
/// - The loop does **not** terminate with an error — it makes another LLM call.
/// - `FinalAnswer` is still emitted when the second LLM call returns text.
#[tokio::test]
async fn test_tool_timeout() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call("tc1", "slow_tool", json!({})))
            .push(MockLlmResponse::text("Timeout handled gracefully.")),
    );

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("slow_tool"),
        // 5 000 ms far exceeds the 50 ms deadline — timeout will fire first.
        MockToolBehavior::Delayed {
            millis: 5_000,
            content: "this should never arrive".into(),
        },
    );

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "run the slow tool".into(),
            }],
            AgentConfig {
                tool_timeout_ms: 50, // 50 ms — fires long before the 5 s delay
                ..AgentConfig::default()
            },
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // The loop must recover: the second LLM call produces the final answer.
    assert_eq!(
        result.unwrap(),
        "Timeout handled gracefully.",
        "loop should complete successfully after timeout"
    );

    // The timed-out tool should appear as a failed completion.
    assert!(
        has_tool_complete_with_success(&events, false),
        "expected a ToolCallComplete with success=false for the timed-out tool"
    );

    assert!(has_final_answer(&events), "missing FinalAnswer");
}

/// **Loop detection**: the model keeps invoking the same tool with the same
/// arguments across multiple iterations.  After `max_protocol_strikes`
/// repetitions the loop must terminate with [`AgentError::LoopDetected`].
///
/// The loop detector computes a signature over tool *names and argument hashes*
/// (not call IDs), so using incrementing IDs does not prevent detection.
#[tokio::test]
async fn test_loop_detection() {
    // Same name + same arguments = same batch signature every iteration.
    let llm = Arc::new(MockLlmPort::new().push_many(
        (0..10).map(|i| MockLlmResponse::tool_call(format!("tc{i}"), "do_thing", json!({}))),
    ));

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("do_thing"),
        MockToolBehavior::Immediate {
            content: "done".into(),
        },
    );

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "go".into(),
            }],
            AgentConfig {
                max_iterations: 10,
                max_protocol_strikes: 2,
                max_stagnation_steps: 100,
                ..AgentConfig::default()
            },
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // Must terminate with LoopDetected.
    assert!(
        matches!(result, Err(AgentError::LoopDetected { .. })),
        "expected LoopDetected, got: {result:?}"
    );

    // The loop emits IterationComplete events before detecting the loop.
    // The detection is returned as a Rust Err — no Error event is emitted
    // (only MaxIterationsReached emits an Error event before returning).
    // At minimum, one IterationComplete should have been emitted.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::IterationComplete { .. })),
        "expected at least one IterationComplete event before loop detection"
    );
}

/// **Context budget pruning**: when the accumulated message history exceeds
/// `context_budget_chars`, the pruning pass must trim old tool messages so
/// the loop can continue rather than aborting.
///
/// This test verifies the end-to-end behaviour: an oversized history is passed
/// in, pruning runs silently during the first iteration, and the loop completes
/// normally with a `FinalAnswer`.
#[tokio::test]
async fn test_context_budget_pruning() {
    // Build a history that far exceeds a small budget.
    // Pattern: System → User → [Assistant(tool_calls) + Tool] × 20 → User
    let mut messages: Vec<AgentMessage> = vec![
        AgentMessage::System {
            content: "You are helpful.".into(),
        },
        AgentMessage::User {
            content: "First question.".into(),
        },
    ];

    for i in 0_u32..20 {
        messages.push(AgentMessage::Assistant {
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: format!("old_tc{i}"),
                name: "search".into(),
                arguments: json!({}),
            }]),
        });
        // 60 chars per result — 20 × 60 = 1 200 chars, well over the 500-char budget.
        messages.push(AgentMessage::Tool {
            tool_call_id: format!("old_tc{i}"),
            content: format!("old tool result {i}: {}", "x".repeat(40)),
        });
    }

    messages.push(AgentMessage::User {
        content: "Final question after a long history.".into(),
    });

    // Single LLM response: no tool calls → FinalAnswer immediately.
    let llm = Arc::new(MockLlmPort::new().push(MockLlmResponse::text(
        "Pruning worked — I can still answer.",
    )));

    // Executor with "search" registered (needed so LLM advertises it),
    // but it won't be called in this test.
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate {
            content: "ok".into(),
        },
    );

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            messages,
            AgentConfig {
                context_budget_chars: 500,
                ..AgentConfig::default()
            },
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // Pruning must not abort the loop — it should complete successfully.
    assert_eq!(
        result.unwrap(),
        "Pruning worked — I can still answer.",
        "loop aborted unexpectedly after context pruning"
    );

    assert!(
        has_final_answer(&events),
        "missing FinalAnswer after pruning"
    );
}
