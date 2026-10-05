//! The loop's third prune site: the recovery from a batch over
//! `max_parallel_tools`, which appends to the history without running a
//! tool.
//!
//! The other two sites, before the first model call and after a tool step,
//! are covered in `unit_agent_loop.rs`.
//!
//! | Test | Guard exercised |
//! |------|-----------------|
//! | [`a_refused_batch_over_budget_is_pruned_and_counted`] | What the recovery appends is pruned to the budget, and the next call's `turn_usage` counts what that dropped |

mod common;

use std::sync::Arc;

use common::event_assertions::collect_events;
use common::mock_llm::{MockLlmPort, MockLlmResponse};
use common::mock_tools::MockToolExecutorPort;
use gglib_agent::AgentLoop;
use gglib_core::domain::agent::{
    AgentEvent, AgentMessage, AssistantContent, ContextReading, ToolCall,
};
use serde_json::json;
use tokio::sync::mpsc;

fn search(id: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: "search".into(),
        arguments: json!({}),
    }
}

/// **Trimmed count, after a refused batch**: a reply asking for more calls
/// than `max_parallel_tools` runs none of them, and the loop appends the
/// assistant message with an error result for each call. On a history
/// already pruned to the budget those put the run over it again, so the
/// next request is pruned further, and its `turn_usage` counts every
/// message missing from it.
#[tokio::test]
async fn a_refused_batch_over_budget_is_pruned_and_counted() {
    let mut history = vec![AgentMessage::user("First question.")];
    for i in 0..20 {
        let id = format!("old{i}");
        history.push(AgentMessage::Assistant {
            content: AssistantContent {
                text: None,
                tool_calls: vec![search(&id)],
            },
        });
        history.push(AgentMessage::Tool {
            tool_call_id: id,
            content: "x".repeat(60),
        });
    }
    history.push(AgentMessage::user("And now?"));
    let had = history.len();

    let refused = MockLlmResponse {
        reasoning: None,
        content: None,
        tool_calls: vec![search("c1"), search("c2")],
        finish_reason: "tool_calls".into(),
    };
    let llm = Arc::new(
        MockLlmPort::new()
            .push(refused)
            .push(MockLlmResponse::text("done")),
    );
    let asked = Arc::clone(&llm);
    let agent = AgentLoop::build(llm, Arc::new(MockToolExecutorPort::new()), None);
    let (tx, rx) = mpsc::channel(64);
    let config = common::for_test(|c| {
        c.max_parallel_tools = 1;
        c.context_budget_chars = 500;
        c.prune_keep_tool_messages = 4;
    });
    agent.run(history, config, tx).await.unwrap();

    let events = collect_events(rx).await;
    let sent = asked.messages_received().await;
    assert_eq!(sent.len(), 2, "two model calls");
    // The second call's request also had the refused batch to carry: its
    // assistant message and one error result for each of its two calls.
    let missing = [had - sent[0].len(), had + 3 - sent[1].len()];
    assert!(missing[0] > 0, "the first request was pruned");
    assert!(missing[1] > missing[0], "and the recovery pruned further");
    let readings: Vec<ContextReading> = events
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TurnUsage(usage) => Some(usage.reading),
            _ => None,
        })
        .collect();
    assert_eq!(
        readings,
        missing.map(|count| ContextReading::new(None, count))
    );
}
