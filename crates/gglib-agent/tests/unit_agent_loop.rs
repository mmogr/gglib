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
//! |------|-----------------|
//! | [`test_iteration_complete_events`] | [`AgentEvent::IterationComplete`] / [`AgentEvent::FinalAnswer`] ordering |
//! | [`test_llm_startup_error_emits_event`] | LLM stream failure → error event before `Err` return |
//! | [`test_empty_tool_filter_exposes_no_tools`] | `build(…, Some([]))` → `EmptyToolExecutor` path |

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentEvent, AgentMessage, ToolDefinition};
use gglib_core::ports::AgentError;
use common::event_assertions::{collect_events, has_error_event};
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use gglib_agent::TOOL_NOT_AVAILABLE_MSG;
use serde_json::json;
use tokio::sync::mpsc;

// =============================================================================
// Tests
// =============================================================================

/// **Iteration / `FinalAnswer` events**: a single-tool iteration should emit
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
        MockToolBehavior::Immediate {
            content: "ok".into(),
        },
    );

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);

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

    let events = collect_events(rx).await;

    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::IterationComplete { iteration: 1, .. })),
        "IterationComplete {{ iteration: 1 }} should be emitted after the first tool-calling iteration"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AgentEvent::FinalAnswer { .. })),
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

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "hello".into(),
            }],
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
        has_error_event(&events),
        "AgentEvent::Error must be emitted before the stream closes on LLM startup failure"
    );
}

/// **Empty tool filter**: `AgentLoop::build` with `Some([])` must route to
/// `EmptyToolExecutor`, not fall through to the unfiltered inner executor.
///
/// Before the fix, `Some([])` was silently treated as "no restriction" and all
/// tools were exposed.  After the fix, `Some([])` means "zero tools allowed":
/// `list_tools` returns empty, and any tool call the model attempts is rejected
/// with `success: false` / "tool filter allows no tools" in the content.
#[tokio::test]
async fn test_empty_tool_filter_exposes_no_tools() {
    // Script: LLM requests a tool on the first call; on the second call (after
    // receiving the failure result) it returns a text answer.
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call("c1", "secret_tool", json!({})))
            .push(MockLlmResponse::text("I could not use any tools.")),
    );
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("secret_tool"),
        MockToolBehavior::Immediate {
            content: "secret data".into(),
        },
    );

    // Pass Some([]) — empty allowlist.
    let agent = AgentLoop::build(llm, Arc::new(executor), Some(HashSet::new()));
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "give me secret data".into(),
            }],
            AgentConfig::default(),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // The loop must complete successfully (the LLM recovered from the failure).
    assert!(
        matches!(result, Ok(ref o) if o.answer == "I could not use any tools."),
        "expected successful final answer after tool rejection, got: {result:?}"
    );

    // The tool call must have been rejected: ToolCallComplete with success=false
    // and a message that names the empty-filter reason.
    let rejection = events.iter().find_map(|e| {
        if let AgentEvent::ToolCallComplete { result } = e {
            Some(result)
        } else {
            None
        }
    });
    let rejection = rejection.expect("ToolCallComplete event must be emitted");
    assert!(
        !rejection.success,
        "tool call must have success=false when empty filter is active"
    );
    assert!(
        rejection.content.contains(TOOL_NOT_AVAILABLE_MSG),
        "rejection message should explain the tool is not available, got: {}",
        rejection.content
    );
}

