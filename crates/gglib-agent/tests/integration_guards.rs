//! Guard integration tests for the backend agentic loop.
//!
//! Covers the three termination guards that abort an unbounded loop:
//! max-iterations limit, loop detection, and text stagnation.
//!
//! All tests are fully in-process using [`MockLlmPort`] and
//! [`MockToolExecutorPort`] — no HTTP server, no llama-server process, no MCP
//! daemon.
//!
//! | Test | Guard exercised |
//! |------|----------------|
//! | [`test_max_iterations_reached`]         | [`AgentConfig::max_iterations`] limit |
//! | [`test_loop_detection`]                 | Repeated tool-call batch → [`AgentError::LoopDetected`] |
//! | [`test_stagnation_detected_integration`] | Repeated text response → [`AgentError::Internal`] |

mod common;

use std::sync::Arc;

use common::event_assertions::has_final_answer;
use common::mock_llm::{collect_events, MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage, ToolCall, ToolDefinition};
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
            AgentConfig {
                max_iterations: 3,
                max_protocol_strikes: None, // disable loop detection for this test
                max_stagnation_steps: None, // disable stagnation for this test
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

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "go".into(),
            }],
            AgentConfig {
                max_iterations: 10,
                max_protocol_strikes: Some(2),
                max_stagnation_steps: None,
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

    // An AgentEvent::Error must be emitted before the stream closes.
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
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
/// with [`AgentError::Internal`] and emit [`AgentEvent::Error`].
///
/// Uses `max_stagnation_steps: Some(2)` so the detector fires after the
/// 3rd identical response (baseline + 2 repeats).
#[tokio::test]
async fn test_stagnation_detected_integration() {
    // Each response includes both text content AND a tool call so the loop
    // does not immediately produce a FinalAnswer and has a chance to stagnate.
    let llm = Arc::new(MockLlmPort::new().push_many((0..5).map(|i| MockLlmResponse {
        content: Some("Thinking...".into()),
        tool_calls: vec![ToolCall {
            id: format!("s{i}"),
            name: "do_thing".into(),
            arguments: json!({}),
        }],
        finish_reason: "tool_calls".into(),
    })));

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
            AgentConfig {
                max_stagnation_steps: Some(2),
                max_protocol_strikes: None,
                max_iterations: 10,
                ..AgentConfig::default()
            },
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert!(
        matches!(result, Err(AgentError::Internal(_))),
        "expected Internal (stagnation), got: {result:?}"
    );

    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
        "AgentEvent::Error must be emitted before stream closes on stagnation"
    );

    assert!(
        !has_final_answer(&events),
        "FinalAnswer must not be emitted when stagnation is detected"
    );
}
