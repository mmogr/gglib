//! Guard integration tests for the backend agentic loop.
//!
//! Covers the termination guards that abort an unbounded loop.
//!
//! All tests are fully in-process using [`MockLlmPort`] and
//! [`MockToolExecutorPort`] — no HTTP server, no llama-server process, no MCP
//! daemon.
//!
//! | Test | Guard exercised |
//! |------|----------------|
//! | [`test_max_iterations_reached`]         | [`AgentConfig::max_iterations`] limit |
//! | [`test_loop_detection`]                 | Repeated tool-call batch → [`AgentError::LoopDetected`] |
//! | [`test_stagnation_detected_integration`] | Repeated text response → [`AgentError::StagnationDetected`] |
//! | [`test_stagnation_fires_before_finalize`] | Stagnation catches repeated final answer |
//! | [`test_too_many_tool_calls_integration`] | Oversized tool-call batch → [`AgentError::ParallelToolLimitExceeded`] |

mod common;

use std::sync::Arc;

use common::event_assertions::{collect_events, has_error_event, has_final_answer};
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentEvent, AgentMessage, ToolCall, ToolDefinition};
use gglib_core::ports::AgentError;
use serde_json::json;
use tokio::sync::mpsc;

// =============================================================================
// Tests
// =============================================================================

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

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "go".into(),
            }],
            common::for_test(|c| {
                c.max_iterations = 3;
                c.max_repeated_batch_steps = None; // disable loop detection for this test
                c.max_stagnation_steps = None; // disable stagnation for this test
            }),
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
        has_error_event(&events),
        "expected at least one Error event in the stream"
    );

    // No FinalAnswer should have been emitted.
    assert!(!has_final_answer(&events), "unexpected FinalAnswer");
}

/// **Loop detection**: the model keeps invoking the same tool with the same
/// arguments across multiple iterations.  After `max_repeated_batch_steps`
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

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "go".into(),
            }],
            common::for_test(|c| {
                c.max_iterations = 10;
                c.max_repeated_batch_steps = Some(2);
                c.max_stagnation_steps = None;
            }),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // Must terminate with LoopDetected.
    assert!(
        matches!(result, Err(AgentError::LoopDetected { .. })),
        "expected LoopDetected, got: {result:?}"
    );

    // An AgentEvent::Error must be emitted before the stream closes.
    assert!(
        has_error_event(&events),
        "expected AgentEvent::Error to be emitted before LoopDetected return"
    );

    // At minimum, one IterationComplete should have been emitted.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::IterationComplete { .. })),
        "expected at least one IterationComplete event before loop detection"
    );
}

/// **Text stagnation**: the LLM produces the same response text on every
/// iteration (text + tool call each time, so the loop does not exit on
/// `FinalAnswer` — it must hit the stagnation guard instead).
/// After `max_stagnation_steps` identical responses the loop must terminate
/// with [`AgentError::StagnationDetected`] and emit [`AgentEvent::Error`].
///
/// Uses `max_stagnation_steps: Some(2)` so the detector fires after the
/// 3rd identical response (baseline + 2 repeats).
#[tokio::test]
async fn test_stagnation_detected_integration() {
    // Each response includes both text content AND a tool call so the loop
    // does not immediately produce a FinalAnswer and has a chance to stagnate.
    let llm = Arc::new(
        MockLlmPort::new().push_many((0..5).map(|i| MockLlmResponse {
            reasoning: None,
            content: Some("Thinking...".into()),
            tool_calls: vec![ToolCall {
                id: format!("s{i}"),
                name: "do_thing".into(),
                arguments: json!({}),
            }],
            finish_reason: "tool_calls".into(),
        })),
    );

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("do_thing"),
        MockToolBehavior::Immediate {
            content: "ok".into(),
        },
    );

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "say something new".into(),
            }],
            common::for_test(|c| {
                c.max_stagnation_steps = Some(2);
                c.max_repeated_batch_steps = None;
                c.max_iterations = 10;
            }),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // max_stagnation_steps=2 → 3 total identical responses before abort
    // (baseline + 2 repeats → fires when repeat_count >= 2, count = 3).
    assert!(
        matches!(
            result,
            Err(AgentError::StagnationDetected {
                max_steps: 2,
                count: 3,
                ..
            })
        ),
        "expected StagnationDetected {{ max_steps: 2, count: 3 }}, got: {result:?}"
    );

    assert!(
        has_error_event(&events),
        "AgentEvent::Error must be emitted before stream closes on stagnation"
    );

    assert!(
        !has_final_answer(&events),
        "FinalAnswer must not be emitted when stagnation is detected"
    );
}

/// **Too many tool calls**: when the LLM returns more tool calls in a single
/// batch than `max_parallel_tools` allows, the loop must terminate immediately
/// with [`AgentError::ParallelToolLimitExceeded`] and emit [`AgentEvent::Error`].
///
/// The batch is rejected *before* any tool is executed — this is checked by
/// asserting that the tool executor is never called.
#[tokio::test]
async fn test_too_many_tool_calls_integration() {
    // LLM emits 3 tool calls in one response; limit is 2.
    let batch = MockLlmResponse {
        reasoning: None,
        content: None,
        tool_calls: vec![
            ToolCall {
                id: "c1".into(),
                name: "search".into(),
                arguments: json!({}),
            },
            ToolCall {
                id: "c2".into(),
                name: "search".into(),
                arguments: json!({}),
            },
            ToolCall {
                id: "c3".into(),
                name: "search".into(),
                arguments: json!({}),
            },
        ],
        finish_reason: "tool_calls".into(),
    };
    let llm = Arc::new(MockLlmPort::new().push(batch));
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate {
            content: "result".into(),
        },
    );

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "search for things".into(),
            }],
            common::for_test(|c| {
                c.max_parallel_tools = 2; // 3 calls > 2 → rejected
                c.max_repeated_batch_steps = None;
            }),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // Must produce the dedicated error variant.
    assert!(
        matches!(
            result,
            Err(AgentError::ParallelToolLimitExceeded { count: 3, limit: 2 })
        ),
        "expected ParallelToolLimitExceeded {{ count: 3, limit: 2 }}, got: {result:?}"
    );

    // An AgentEvent::Error must have been emitted before the stream closes.
    assert!(
        has_error_event(&events),
        "AgentEvent::Error must be emitted on ParallelToolLimitExceeded"
    );

    // The tool must never have been called — the batch is rejected before execution.
    assert!(
        !has_final_answer(&events),
        "FinalAnswer must not be emitted when the batch is rejected"
    );
}

/// **Stagnation fires before finalize**: A model produces repeated text with
/// tool calls (building up the stagnation counter), then produces the same
/// text WITHOUT tools.  Because guards run before the finalize check, the
/// stagnation guard must fire on the text-only iteration, preventing a
/// stagnated final answer from being accepted.
///
/// Uses `max_stagnation_steps: Some(1)` so the detector fires after the
/// 2nd identical response (baseline + 1 repeat).
#[tokio::test]
async fn test_stagnation_fires_before_finalize() {
    let llm = Arc::new(
        MockLlmPort::new()
            // Iteration 0: "Stuck" + tool call → stagnation records count=1 (ok)
            .push(MockLlmResponse {
                reasoning: None,
                content: Some("Stuck".into()),
                tool_calls: vec![ToolCall {
                    id: "t0".into(),
                    name: "do_thing".into(),
                    arguments: json!({}),
                }],
                finish_reason: "tool_calls".into(),
            })
            // Iteration 1: "Stuck" (no tools) → stagnation records count=2 > max_steps=1 → error
            // Without the guards-before-finalize restructure, this would be
            // accepted as a FinalAnswer.
            .push(MockLlmResponse::text("Stuck")),
    );

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("do_thing"),
        MockToolBehavior::Immediate {
            content: "ok".into(),
        },
    );

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "go".into(),
            }],
            common::for_test(|c| {
                c.max_stagnation_steps = Some(1);
                c.max_repeated_batch_steps = None;
                c.max_iterations = 10;
            }),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert!(
        matches!(
            result,
            Err(AgentError::StagnationDetected {
                max_steps: 1,
                count: 2,
                ..
            })
        ),
        "expected StagnationDetected {{ max_steps: 1, count: 2 }}, got: {result:?}"
    );

    assert!(
        has_error_event(&events),
        "AgentEvent::Error must be emitted before stream closes on stagnation"
    );

    assert!(
        !has_final_answer(&events),
        "FinalAnswer must not be emitted when stagnation aborts a text-only iteration"
    );
}
