//! Maps inner [`AgentEvent`]s to tagged [`CouncilEvent`] debate variants.
//!
//! The bridge sits between a delegated `AgentLoop` channel and the outer
//! council event channel.  It tags every event with `node_id`, `agent_id`,
//! and `round`, converting the generic agent stream into debate-specific
//! wire events.
//!
//! This module contains **no business logic** — it is a pure, lossless
//! event transformer.

use tokio::sync::mpsc;
use tracing::debug;

use gglib_core::AgentEvent;
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::task_graph::DebateAgent;

use super::state::extract_core_claim;

/// Consume all events from an `AgentLoop` channel, map each to its
/// [`CouncilEvent::Debate*`] counterpart, and forward to the council sender.
///
/// Returns the final answer text (if one was emitted) so the debate runner
/// can record it as an [`AgentContribution`](super::state::AgentContribution).
///
/// # Behaviour on channel closure
///
/// When the inner `AgentLoop` finishes (or is terminated by stagnation /
/// loop detection), it drops its `Sender`, which closes the channel.
/// This function returns `None` if no `FinalAnswer` was received — the
/// runner treats this as a truncated contribution and moves on.
pub async fn bridge_agent_events(
    node_id: &str,
    agent: &DebateAgent,
    round: u32,
    mut agent_rx: mpsc::Receiver<AgentEvent>,
    council_tx: &mpsc::Sender<CouncilEvent>,
) -> Option<String> {
    let id = &agent.id;
    let mut final_content: Option<String> = None;

    while let Some(event) = agent_rx.recv().await {
        let council_event = match event {
            AgentEvent::TextDelta { content } => CouncilEvent::DebateAgentTextDelta {
                node_id: node_id.to_owned(),
                agent_id: id.clone(),
                delta: content,
            },

            AgentEvent::ReasoningDelta { content } => CouncilEvent::DebateAgentReasoningDelta {
                node_id: node_id.to_owned(),
                agent_id: id.clone(),
                delta: content,
            },

            AgentEvent::ToolCallStart {
                tool_call,
                display_name,
                args_summary,
            } => CouncilEvent::DebateAgentToolCallStart {
                node_id: node_id.to_owned(),
                agent_id: id.clone(),
                tool_call,
                display_name,
                args_summary,
            },

            AgentEvent::ToolCallComplete {
                result,
                display_name,
                duration_display,
                ..
            } => CouncilEvent::DebateAgentToolCallComplete {
                node_id: node_id.to_owned(),
                agent_id: id.clone(),
                result,
                display_name,
                duration_display,
            },

            AgentEvent::FinalAnswer { content } => {
                final_content = Some(content);
                // Don't emit a council event here — the runner emits
                // `DebateAgentTurnComplete` after recording the contribution.
                continue;
            }

            AgentEvent::PromptProgress {
                processed,
                total,
                cached,
                time_ms,
            } => CouncilEvent::DebateAgentProgress {
                node_id: node_id.to_owned(),
                agent_id: id.clone(),
                processed,
                total,
                cached,
                time_ms,
            },

            // Iteration boundaries are internal to the agent loop.
            // Warning events — not part of the debate wire format.
            AgentEvent::IterationComplete { .. } | AgentEvent::SystemWarning { .. } => continue,

            // Agent-level errors are logged but don't terminate the debate.
            AgentEvent::Error { message } => {
                debug!(agent_id = %id, round, %message, "agent error during debate turn");
                continue;
            }
        };

        // Best-effort send — if the council receiver is dropped, stop forwarding.
        if council_tx.send(council_event).await.is_err() {
            debug!(agent_id = %id, round, "council channel closed, draining agent events");
            break;
        }
    }

    final_content
}

/// Emit the [`CouncilEvent::DebateAgentTurnComplete`] event, extracting the
/// core claim from the response content.
pub async fn emit_turn_complete(
    node_id: &str,
    agent: &DebateAgent,
    round: u32,
    content: &str,
    council_tx: &mpsc::Sender<CouncilEvent>,
) {
    let core_claim = extract_core_claim(content);
    let _ = council_tx
        .send(CouncilEvent::DebateAgentTurnComplete {
            node_id: node_id.to_owned(),
            agent_id: agent.id.clone(),
            round: round + 1, // 1-based in the wire format
            final_text: content.to_owned(),
        })
        .await;
    // core_claim is used in state, not the event (not part of wire schema)
    let _ = core_claim; // suppress unused warning; used via extract_core_claim at push site
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::{ToolCall, ToolResult};

    fn test_agent() -> DebateAgent {
        DebateAgent {
            id: "test-1".into(),
            name: "Tester".into(),
            color: "#fff".into(),
            persona: "p".into(),
            perspective: "v".into(),
            contentiousness: 0.5,
            tool_filter: None,
        }
    }

    #[tokio::test]
    async fn text_deltas_forwarded_with_node_and_agent_id() {
        let agent = test_agent();
        let (agent_tx, agent_rx) = mpsc::channel(32);
        let (council_tx, mut council_rx) = mpsc::channel(32);

        agent_tx
            .send(AgentEvent::TextDelta {
                content: "hello".into(),
            })
            .await
            .unwrap();
        agent_tx
            .send(AgentEvent::FinalAnswer {
                content: "hello world".into(),
            })
            .await
            .unwrap();
        drop(agent_tx);

        let answer = bridge_agent_events("node-1", &agent, 0, agent_rx, &council_tx).await;
        assert_eq!(answer.as_deref(), Some("hello world"));

        let event = council_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            CouncilEvent::DebateAgentTextDelta { node_id, agent_id, delta }
                if node_id == "node-1" && agent_id == "test-1" && delta == "hello"
        ));
    }

    #[tokio::test]
    async fn tool_events_forwarded() {
        let agent = test_agent();
        let (agent_tx, agent_rx) = mpsc::channel(32);
        let (council_tx, mut council_rx) = mpsc::channel(32);

        agent_tx
            .send(AgentEvent::ToolCallStart {
                tool_call: ToolCall {
                    id: "c1".into(),
                    name: "search".into(),
                    arguments: serde_json::json!({}),
                },
                display_name: "Web Search".into(),
                args_summary: Some("query".into()),
            })
            .await
            .unwrap();
        agent_tx
            .send(AgentEvent::ToolCallComplete {
                tool_name: "search".into(),
                result: ToolResult {
                    tool_call_id: "c1".into(),
                    content: "results".into(),
                    success: true,
                },
                wait_ms: 0,
                execute_duration_ms: 100,
                display_name: "Web Search".into(),
                duration_display: "100ms".into(),
            })
            .await
            .unwrap();
        agent_tx
            .send(AgentEvent::FinalAnswer {
                content: "done".into(),
            })
            .await
            .unwrap();
        drop(agent_tx);

        bridge_agent_events("node-1", &agent, 1, agent_rx, &council_tx).await;

        let e1 = council_rx.recv().await.unwrap();
        assert!(
            matches!(e1, CouncilEvent::DebateAgentToolCallStart { agent_id, .. } if agent_id == "test-1")
        );

        let e2 = council_rx.recv().await.unwrap();
        assert!(
            matches!(e2, CouncilEvent::DebateAgentToolCallComplete { agent_id, .. } if agent_id == "test-1")
        );
    }

    #[tokio::test]
    async fn iteration_and_error_events_skipped() {
        let agent = test_agent();
        let (agent_tx, agent_rx) = mpsc::channel(32);
        let (council_tx, mut council_rx) = mpsc::channel(32);

        agent_tx
            .send(AgentEvent::IterationComplete {
                iteration: 0,
                tool_calls: 2,
            })
            .await
            .unwrap();
        agent_tx
            .send(AgentEvent::Error {
                message: "stagnation".into(),
            })
            .await
            .unwrap();
        agent_tx
            .send(AgentEvent::FinalAnswer {
                content: "recovered".into(),
            })
            .await
            .unwrap();
        drop(agent_tx);

        let answer = bridge_agent_events("node-1", &agent, 0, agent_rx, &council_tx).await;
        assert_eq!(answer.as_deref(), Some("recovered"));

        // Channel should be empty — iteration/error events were skipped.
        assert!(council_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn no_final_answer_returns_none() {
        let agent = test_agent();
        let (agent_tx, agent_rx) = mpsc::channel(32);
        let (council_tx, _council_rx) = mpsc::channel(32);

        agent_tx
            .send(AgentEvent::TextDelta {
                content: "partial".into(),
            })
            .await
            .unwrap();
        drop(agent_tx);

        let answer = bridge_agent_events("node-1", &agent, 0, agent_rx, &council_tx).await;
        assert!(answer.is_none());
    }

    #[tokio::test]
    async fn prompt_progress_forwarded_as_debate_agent_progress() {
        // Regression test: PromptProgress used to be silently dropped here
        // (`continue`d alongside IterationComplete/SystemWarning), which
        // meant debate-mode requests never showed prompt-processing
        // progress on the proxy dashboard. It must now be forwarded.
        let agent = test_agent();
        let (agent_tx, agent_rx) = mpsc::channel(32);
        let (council_tx, mut council_rx) = mpsc::channel(32);

        agent_tx
            .send(AgentEvent::PromptProgress {
                processed: 128,
                total: 512,
                cached: 64,
                time_ms: 42,
            })
            .await
            .unwrap();
        agent_tx
            .send(AgentEvent::FinalAnswer {
                content: "done".into(),
            })
            .await
            .unwrap();
        drop(agent_tx);

        bridge_agent_events("node-1", &agent, 0, agent_rx, &council_tx).await;

        let event = council_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            CouncilEvent::DebateAgentProgress {
                node_id,
                agent_id,
                processed: 128,
                total: 512,
                cached: 64,
                time_ms: 42,
            } if node_id == "node-1" && agent_id == "test-1"
        ));
    }
}
