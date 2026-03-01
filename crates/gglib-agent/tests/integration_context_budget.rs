//! Context-budget pruning integration test for the backend agentic loop.
//!
//! Verifies the end-to-end behaviour of `prune_for_budget`: when the
//! accumulated message history exceeds `context_budget_chars`, the pruning
//! pass silently trims old tool messages so the loop can continue rather
//! than aborting.
//!
//! All tests are fully in-process using [`MockLlmPort`] and
//! [`MockToolExecutorPort`] — no HTTP server, no llama-server process, no MCP
//! daemon.
//!
//! | Test | Scenario |
//! |------|----------|
//! | [`test_context_budget_pruning`] | Oversized history → pruning → loop continues |

mod common;

use std::sync::Arc;

use common::event_assertions::has_final_answer;
use common::mock_llm::{collect_events, MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentConfig, AgentMessage, ToolCall, ToolDefinition};
use serde_json::json;
use tokio::sync::mpsc;

// =============================================================================
// Tests
// =============================================================================

/// **Context budget pruning**: when the accumulated message history exceeds
/// `context_budget_chars`, the pruning pass must trim old tool messages so
/// the loop can continue rather than aborting.
///
/// This test verifies:
/// 1. The agent completes with a `FinalAnswer` despite an oversized history.
/// 2. The LLM actually received fewer messages than the original history
///    (i.e. pruning was not a no-op).
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
    let llm = Arc::new(
        MockLlmPort::new().push(MockLlmResponse::text("Pruning worked — I can still answer.")),
    );
    // Keep a handle so we can inspect what messages the LLM actually received.
    let llm_handle = Arc::clone(&llm);

    // Executor with "search" registered (needed so LLM advertises it),
    // but it won't be called in this test.
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate {
            content: "ok".into(),
        },
    );

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
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
        result.unwrap().answer,
        "Pruning worked — I can still answer.",
        "loop aborted unexpectedly after context pruning"
    );

    assert!(
        has_final_answer(&events),
        "missing FinalAnswer after pruning"
    );

    // Verify that pruning actually reduced the context: the LLM was called
    // exactly once, and it received fewer than the original 43 messages.
    // (2 initial + 40 assistant/tool pairs + 1 final user = 43)
    let received = llm_handle.messages_received().await;
    assert_eq!(received.len(), 1, "expected exactly one LLM call");
    assert!(
        received[0].len() < 43,
        "pruning must reduce the message count below the original 43; got {}",
        received[0].len()
    );
}
