//! Happy-path integration tests for the backend agentic loop.
//!
//! Covers the core LLM→tool→LLM round-trips that succeed with a `FinalAnswer`:
//! single tool call, parallel tool batch, and graceful tool-timeout recovery.
//!
//! All tests are fully in-process using [`MockLlmPort`] and
//! [`MockToolExecutorPort`] — no HTTP server, no llama-server process, no MCP
//! daemon.
//!
//! | Test | Scenario |
//! |------|----------|
//! | [`test_simple_tool_call_cycle`] | One tool call → final answer |
//! | [`test_parallel_tool_calls`]    | Three tools in one batch → final answer |
//! | [`test_tool_timeout`]           | Slow tool times out; loop recovers |
//! | [`test_reasoning_delta_emitted`]| Reasoning block forwarded as event |
//! | [`test_both_text_and_tool_calls_in_history`] | Text preamble + tool calls preserved |

mod common;

use std::sync::Arc;

use common::event_assertions::{
    collect_events, has_final_answer, has_tool_complete_with_success, has_tool_start,
};
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{
    AgentConfig, AgentEvent, AgentMessage, AssistantContent, ToolCall, ToolDefinition,
};
use serde_json::json;
use tokio::sync::mpsc;

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
    let log = Arc::clone(&executor.call_log);

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
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

    assert_eq!(result.unwrap().answer, "Here are the results.");

    // Tool was called exactly once with the right name.
    let calls = log.lock().await.clone();
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
/// Verifies that `execute_tools_parallel` dispatches all calls and that the
/// agent loop correctly builds the follow-up conversation.
#[tokio::test]
async fn test_parallel_tool_calls() {
    let batch = MockLlmResponse {
        reasoning: None,
        content: None,
        tool_calls: vec![
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
        ],
        finish_reason: "tool_calls".into(),
    };

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
    let log = Arc::clone(&executor.call_log);

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(128);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "Search three topics".into(),
            }],
            common::for_test(|c| {
                c.max_parallel_tools = 3;
            }),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert_eq!(result.unwrap().answer, "All done.");

    // All three tool calls were executed.
    let calls = log.lock().await.clone();
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

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "run the slow tool".into(),
            }],
            common::for_test(|c| {
                c.tool_timeout_ms = 50;
            }),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    // The loop must recover: the second LLM call produces the final answer.
    assert_eq!(
        result.unwrap().answer,
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

/// **`ReasoningDelta` forwarded**: the LLM emits a reasoning block before the
/// final text answer.  The agent loop must forward the reasoning as
/// [`AgentEvent::ReasoningDelta`] and still produce the [`AgentEvent::FinalAnswer`].
///
/// Exercises the `LlmStreamEvent::ReasoningDelta` → `AgentEvent::ReasoningDelta`
/// path through `stream_collector` without any tool calls.
#[tokio::test]
async fn test_reasoning_delta_emitted() {
    let llm = Arc::new(
        MockLlmPort::new().push(MockLlmResponse::text_with_reasoning(
            "Let me think about this carefully.",
            "The answer is 42.",
        )),
    );

    let executor = MockToolExecutorPort::new();
    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            vec![AgentMessage::User {
                content: "What is the meaning of life?".into(),
            }],
            AgentConfig::default(),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert_eq!(result.unwrap().answer, "The answer is 42.");

    // The reasoning delta must appear in the event stream.
    assert!(
        events.iter().any(|e| matches!(
            e,
            AgentEvent::ReasoningDelta { content }
            if content == "Let me think about this carefully."
        )),
        "expected AgentEvent::ReasoningDelta with the scripted reasoning text; events: {events:?}"
    );

    assert!(has_final_answer(&events), "missing FinalAnswer");
}

/// **Both text and tool calls preserved**: when the LLM emits a text preamble
/// alongside tool calls, the returned `history` must contain an
/// `AssistantContent` with **both** `text = Some(...)` and a non-empty
/// `tool_calls` vec — verifying the struct fields survive the full loop.
#[tokio::test]
async fn test_both_text_and_tool_calls_in_history() {
    let llm = Arc::new(
        MockLlmPort::new()
            // Iteration 1: text preamble + tool call
            .push(MockLlmResponse {
                reasoning: None,
                content: Some("Let me search for that.".into()),
                tool_calls: vec![ToolCall {
                    id: "tc1".into(),
                    name: "search".into(),
                    arguments: json!({"q": "rust"}),
                }],
                finish_reason: "tool_calls".into(),
            })
            // Iteration 2: final answer (text only)
            .push(MockLlmResponse::text("Found it!")),
    );

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate {
            content: "result".into(),
        },
    );

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);

    let output = agent
        .run(
            vec![AgentMessage::User {
                content: "Look it up".into(),
            }],
            AgentConfig::default(),
            tx,
        )
        .await
        .expect("agent loop should succeed");

    let _events = collect_events(rx).await;

    // Find the mid-loop assistant message that has both text and tool calls.
    let both_msg = output.history.iter().find(|m| {
        matches!(
            m,
            AgentMessage::Assistant {
                content: AssistantContent {
                    text: Some(_),
                    tool_calls,
                },
            } if !tool_calls.is_empty()
        )
    });

    assert!(
        both_msg.is_some(),
        "history must contain an assistant message with both text and tool_calls; history: {:?}",
        output.history
    );

    if let Some(AgentMessage::Assistant { content }) = both_msg {
        assert_eq!(content.text.as_deref(), Some("Let me search for that."));
        assert_eq!(content.tool_calls.len(), 1);
        assert_eq!(content.tool_calls[0].name, "search");
    }
}
