//! Maps inner [`AgentEvent`]s to tagged [`CouncilEvent`] variants.
//!
//! The bridge sits between a delegated `AgentLoop` channel and the outer
//! council event channel.  It tags every event with `agent_id` and `round`,
//! converting the generic agent stream into the council-specific wire format.
//!
//! This module contains **no business logic** — it is a pure, lossless
//! event transformer.

use tokio::sync::mpsc;
use tracing::debug;

use gglib_core::AgentEvent;

use super::config::CouncilAgent;
use super::events::CouncilEvent;
use super::state::extract_core_claim;

/// Consume all events from an `AgentLoop` channel, map each to its
/// [`CouncilEvent`] counterpart, and forward to the council sender.
///
/// Returns the final answer text (if one was emitted) so the orchestrator
/// can record it as an [`AgentContribution`](super::state::AgentContribution).
///
/// # Behaviour on channel closure
///
/// When the inner `AgentLoop` finishes (or is terminated by stagnation /
/// loop detection), it drops its `Sender`, which closes the channel.
/// This function returns `None` if no `FinalAnswer` was received — the
/// orchestrator treats this as a truncated contribution and moves on.
pub async fn bridge_agent_events(
    agent: &CouncilAgent,
    round: u32,
    mut agent_rx: mpsc::Receiver<AgentEvent>,
    council_tx: &mpsc::Sender<CouncilEvent>,
) -> Option<String> {
    let id = &agent.id;
    let mut final_content: Option<String> = None;

    while let Some(event) = agent_rx.recv().await {
        let council_event = match event {
            AgentEvent::TextDelta { content } => CouncilEvent::AgentTextDelta {
                agent_id: id.clone(),
                delta: content,
            },

            AgentEvent::ReasoningDelta { content } => CouncilEvent::AgentReasoningDelta {
                agent_id: id.clone(),
                delta: content,
            },

            AgentEvent::ToolCallStart {
                tool_call,
                display_name,
                args_summary,
            } => CouncilEvent::AgentToolCallStart {
                agent_id: id.clone(),
                tool_call,
                display_name,
                args_summary,
            },

            AgentEvent::ToolCallComplete {
                tool_name,
                result,
                display_name,
                duration_display,
                ..
            } => CouncilEvent::AgentToolCallComplete {
                agent_id: id.clone(),
                tool_name,
                result,
                display_name,
                duration_display,
            },

            AgentEvent::FinalAnswer { content } => {
                final_content = Some(content);
                // Don't emit a council event here — the orchestrator emits
                // `AgentTurnComplete` after recording the contribution.
                continue;
            }

            // Iteration boundaries are internal to the agent loop; they
            // don't map to a council-level concept.
            AgentEvent::IterationComplete { .. } => continue,

            // Agent-level errors are logged but don't terminate the council.
            // The orchestrator handles the `None` return for final_content.
            AgentEvent::Error { message } => {
                debug!(agent_id = %id, round, %message, "agent error during council turn");
                continue;
            }
        };

        // Best-effort send — if the council receiver is dropped (e.g. client
        // disconnected), we stop forwarding but still drain to completion.
        if council_tx.send(council_event).await.is_err() {
            debug!(agent_id = %id, round, "council channel closed, draining agent events");
            break;
        }
    }

    final_content
}

/// Emit the [`CouncilEvent::AgentTurnComplete`] event, extracting the
/// core claim from the response content.
///
/// This is called by the orchestrator after `bridge_agent_events` returns,
/// ensuring the complete content is available for core claim parsing.
pub async fn emit_turn_complete(
    agent: &CouncilAgent,
    round: u32,
    content: &str,
    council_tx: &mpsc::Sender<CouncilEvent>,
) {
    let core_claim = extract_core_claim(content);
    let event = CouncilEvent::AgentTurnComplete {
        agent_id: agent.id.clone(),
        content: content.to_owned(),
        round,
        core_claim,
    };
    let _ = council_tx.send(event).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::{ToolCall, ToolResult};

    fn test_agent() -> CouncilAgent {
        CouncilAgent {
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
    async fn text_deltas_forwarded_with_agent_id() {
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

        let answer = bridge_agent_events(&agent, 0, agent_rx, &council_tx).await;
        assert_eq!(answer.as_deref(), Some("hello world"));

        let event = council_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            CouncilEvent::AgentTextDelta { agent_id, delta }
                if agent_id == "test-1" && delta == "hello"
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

        bridge_agent_events(&agent, 1, agent_rx, &council_tx).await;

        let e1 = council_rx.recv().await.unwrap();
        assert!(
            matches!(e1, CouncilEvent::AgentToolCallStart { agent_id, .. } if agent_id == "test-1")
        );

        let e2 = council_rx.recv().await.unwrap();
        assert!(
            matches!(e2, CouncilEvent::AgentToolCallComplete { agent_id, .. } if agent_id == "test-1")
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

        let answer = bridge_agent_events(&agent, 0, agent_rx, &council_tx).await;
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

        let answer = bridge_agent_events(&agent, 0, agent_rx, &council_tx).await;
        assert!(answer.is_none());
    }

    #[tokio::test]
    async fn emit_turn_complete_with_core_claim() {
        let agent = test_agent();
        let (council_tx, mut council_rx) = mpsc::channel(32);

        emit_turn_complete(&agent, 1, "Analysis.\nCORE CLAIM: Test claim.", &council_tx).await;

        let event = council_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            CouncilEvent::AgentTurnComplete { core_claim: Some(c), round: 1, .. }
                if c == "Test claim."
        ));
    }

    #[tokio::test]
    async fn emit_turn_complete_without_core_claim() {
        let agent = test_agent();
        let (council_tx, mut council_rx) = mpsc::channel(32);

        emit_turn_complete(&agent, 0, "No marker here.", &council_tx).await;

        let event = council_rx.recv().await.unwrap();
        assert!(matches!(
            event,
            CouncilEvent::AgentTurnComplete {
                core_claim: None,
                ..
            }
        ));
    }
}
