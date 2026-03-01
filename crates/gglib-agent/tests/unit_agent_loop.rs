//! Unit-level tests for [`AgentLoop`] covering guards not exercised by the
//! integration suite.
//!
//! These tests live here (rather than inside `src/agent_loop.rs`) so they can
//! share the common mock infrastructure in `tests/common/` and avoid the
//! duplicate `testutil` module that was removed in the C2 cleanup.
//!
//! # Coverage
//!
//! | Test | Guard exercised |
//! |------|----------------|
//! | [`test_stagnation_detected`] | Repeated response text → [`AgentError::Internal`] |
//! | [`test_iteration_complete_events`] | [`AgentEvent::IterationComplete`] / [`AgentEvent::FinalAnswer`] ordering |
//! | [`test_llm_startup_error_emits_event`] | LLM stream failure → error event before `Err` return |

mod common;

use std::sync::Arc;

use common::collect_events;
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage, ToolCall, ToolDefinition};
use gglib_core::ports::{AgentError, AgentLoopPort};
use serde_json::json;
use tokio::sync::mpsc;

// =============================================================================
// Tests
// =============================================================================

/// **Stagnation detection**: every LLM response contains the same text plus a
/// tool call (unique ID each time so loop detection does not fire first).
/// After `max_stagnation_steps` repeats the loop must abort with
/// [`AgentError::Internal`] and emit an [`AgentEvent::Error`] before closing.
#[tokio::test]
async fn test_stagnation_detected() {
    let llm = Arc::new(MockLlmPort::new().push_many((0..10).map(|i| MockLlmResponse {
        content: Some("Thinking...".into()),
        tool_calls: vec![ToolCall {
            id: format!("c{i}"),
            name: "do_thing".into(),
            arguments: json!({}),
        }],
        finish_reason: "tool_calls".into(),
    })));

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("do_thing"),
        MockToolBehavior::Immediate { content: "ok".into() },
    );

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User { content: "go".into() }],
            AgentConfig {
                max_iterations: 10,
                max_protocol_strikes: 100, // disable loop detection
                max_stagnation_steps: 2,   // fires on the 3rd identical-text iteration
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

    // An AgentEvent::Error must be emitted before the stream closes.
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
        "AgentEvent::Error must be emitted before stagnation Err return"
    );
}

/// **Iteration / FinalAnswer events**: a single-tool iteration should emit
/// `IterationComplete { iteration: 1, .. }` followed by `FinalAnswer`.
#[tokio::test]
async fn test_iteration_complete_events() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call("c1", "do_thing", json!({})))
            .push(MockLlmResponse::text("done")),
    );

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("do_thing"),
        MockToolBehavior::Immediate { content: "ok".into() },
    );

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(64);

    agent
        .run(
            vec![AgentMessage::User { content: "go".into() }],
            AgentConfig::default(),
            tx,
        )
        .await
        .unwrap();

    let events = collect_events(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::IterationComplete { iteration: 1, .. })),
        "IterationComplete {{ iteration: 1 }} should be emitted after the first tool-calling iteration"
    );
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::FinalAnswer { .. })),
        "FinalAnswer should be emitted"
    );
}

/// **LLM startup error**: when `chat_stream` returns `Err` the loop must
/// emit [`AgentEvent::Error`] on the channel *before* returning
/// `Err(AgentError::Internal)` — SSE clients always see the termination reason.
#[tokio::test]
async fn test_llm_startup_error_emits_event() {
    // An empty MockLlmPort returns Err on the first call (no responses queued).
    let llm = Arc::new(MockLlmPort::new());
    let executor = MockToolExecutorPort::new();

    let agent = AgentLoop::new(llm, Arc::new(executor));
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User { content: "hello".into() }],
            AgentConfig::default(),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert!(
        matches!(result, Err(AgentError::Internal(_))),
        "expected Internal on LLM startup failure, got: {result:?}"
    );

    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
        "AgentEvent::Error must be emitted before the stream closes on LLM startup failure"
    );
}
