//! Context-budget pruning integration tests for the backend agentic loop.
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
//! | [`test_context_budget_pruning`]           | Oversized history → pruning → loop continues |
//! | [`test_context_budget_pruning_two_iters`] | Pruning across two iterations — verifies `running_chars` is kept correct after Pass 2 |

mod common;

use std::sync::Arc;

use common::event_assertions::{collect_events, has_final_answer};
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{AgentMessage, AssistantContent, ToolCall, ToolDefinition};
use serde_json::json;
use tokio::sync::mpsc;

// =============================================================================
// Shared test utilities
// =============================================================================

/// Build a history of `n_pairs` completed tool-call rounds, preceded by a
/// `System` message and an initial `User` message.
///
/// Returns (in order):
///   - `System("You are helpful.")` 
///   - `User("First question.")`
///   - for each `i` in `0..n_pairs`: `Assistant(tool_calls=[old_tc{i}])` then `Tool(old_tc{i})`
///
/// Each `Tool` body is `"old result {i}: " + "x" × 40` (~58 chars), so
/// 20 pairs give ~1 160 tool-message chars — well above any reasonable 500-char
/// test budget.  The caller must append a final `User` message (or any other
/// messages) before passing the history to the agent.
fn build_long_history(n_pairs: u32) -> Vec<AgentMessage> {
    let mut messages = vec![
        AgentMessage::System { content: "You are helpful.".into() },
        AgentMessage::User   { content: "First question.".into() },
    ];
    for i in 0..n_pairs {
        messages.push(AgentMessage::Assistant {
            content: AssistantContent::ToolCalls(vec![ToolCall {
                id: format!("old_tc{i}"),
                name: "search".into(),
                arguments: json!({}),
            }]),
        });
        messages.push(AgentMessage::Tool {
            tool_call_id: format!("old_tc{i}"),
            content: format!("old result {i}: {}", "x".repeat(40)),
        });
    }
    messages
}

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
    // (20 × ~58 tool-message chars ≈ 1 160 chars, well over the 500-char budget)
    let mut messages = build_long_history(20);
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
            common::for_test(|c| { c.context_budget_chars = 500; }),
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

/// **Two-iteration pruning** — verifies that `running_chars` is correctly
/// updated after Pass 2.
///
/// Before the fix for the `running_chars` staleness bug, Pass 2 would prune
/// messages but leave the internal counter pointing at the pre-prune total.
/// On the very next iteration, even a tiny addition (one new tool result) would
/// push the counter over budget and re-trigger Pass 2 unnecessarily, silently
/// over-pruning history.
///
/// This test exercises that path explicitly:
/// 1. Start with a history that forces Pass 2.
/// 2. First LLM call → tool call (loop continues, tool result is appended).
/// 3. Second LLM call → final text answer.
///
/// A correct implementation completes with the expected answer and makes
/// exactly two LLM calls.  If the counter were stale after Pass 2 the second
/// call would see a wrongly-pruned message list but would still complete — the
/// message-count assertion on the second call detects over-pruning.
#[tokio::test]
async fn test_context_budget_pruning_two_iters() {
    // Build a large history that triggers Pass 2 on the first prune.
    let mut messages = build_long_history(20);
    messages.push(AgentMessage::User { content: "Second question.".into() });

    // Two LLM responses: first a tool call, then a final answer.
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call("live_tc0", "search", json!({})))
            .push(MockLlmResponse::text("Two-pass pruning worked.")),
    );
    let llm_handle = Arc::clone(&llm);

    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("search"),
        MockToolBehavior::Immediate { content: "live result".into() },
    );

    let agent = AgentLoop::build(llm, Arc::new(executor), None);
    let (tx, rx) = mpsc::channel(64);

    let result = agent
        .run(
            messages,
            // Budget of 600 chars: forces pruning on the initial ~1 600-char history while
            // leaving enough room for the ~35-char new tool-call+result pair added after
            // the first iteration so the second call is not over-pruned.
            common::for_test(|c| { c.context_budget_chars = 600; }),
            tx,
        )
        .await;

    let events = collect_events(rx).await;

    assert_eq!(
        result.unwrap().answer,
        "Two-pass pruning worked.",
        "loop aborted unexpectedly on second iteration after context pruning"
    );
    assert!(has_final_answer(&events), "missing FinalAnswer after two-iteration pruning");

    let received = llm_handle.messages_received().await;
    assert_eq!(received.len(), 2, "expected exactly two LLM calls");

    // Both calls must have received a pruned (reduced) history.
    let original_count = 2 + 20 * 2 + 1; // = 43
    assert!(
        received[0].len() < original_count,
        "first call: pruning must reduce message count below {original_count}; got {}",
        received[0].len()
    );
    // The second call adds at least two new messages (assistant tool-call +
    // tool result).  If running_chars was stale after Pass 2, the second call
    // would see an aggressively pruned list shorter than (pruned_first + 2).
    // A correct implementation retains the messages from the first call's
    // result plus the new tool exchange — the +2 assertion would fail if
    // over-pruning dropped those messages.
    assert!(
        received[1].len() >= received[0].len() + 2,
        "second call must include at least two new messages (tool-call + result) \
         beyond the first call's message list; first={}, second={}",
        received[0].len(),
        received[1].len()
    );
}
