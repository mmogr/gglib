//! Synthesis pass for a debate node.
//!
//! After all debate rounds complete, the synthesiser assembles the full debate
//! transcript and runs a single-iteration [`AgentLoop`](crate::AgentLoop)
//! to produce a unified answer that integrates all agent positions.
//!
//! Returns the synthesis text as a `String` so the caller can record it as
//! the node's output.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;

use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::task_graph::DebateConfig;
use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage};

use crate::AgentLoop;

use super::history::format_synthesis_transcript;
use super::prompts::SYNTHESIS_PROMPT;
use super::state::DebateState;

/// Run the synthesis phase: build the transcript, call a single-iteration
/// [`AgentLoop`], and emit `DebateSynthesisStarted` /
/// `DebateSynthesisTextDelta` / `DebateSynthesisComplete`.
///
/// Returns the synthesis text, or an empty string on failure.
pub(super) async fn run_synthesis(
    node_id: &str,
    topic: &str,
    config: &DebateConfig,
    agent_config: AgentConfig,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    state: &DebateState,
    council_tx: &mpsc::Sender<CouncilEvent>,
) -> String {
    // Announce synthesis start.
    let _ = council_tx
        .send(CouncilEvent::DebateSynthesisStarted {
            node_id: node_id.to_owned(),
        })
        .await;

    let transcript = format_synthesis_transcript(state);
    let guidance = config
        .synthesis_guidance
        .as_deref()
        .unwrap_or("Provide an actionable synthesis.");

    #[allow(clippy::literal_string_with_formatting_args)]
    let synthesis_prompt = SYNTHESIS_PROMPT
        .replace("{agent_count}", &config.agents.len().to_string())
        .replace("{topic}", topic)
        .replace("{transcript}", &transcript)
        .replace("{synthesis_guidance}", guidance);

    let synth_messages = vec![
        AgentMessage::System {
            content: synthesis_prompt,
        },
        AgentMessage::User {
            content: topic.to_owned(),
        },
    ];

    // Synthesis uses a restricted config — no tools needed, single iteration.
    let synth_loop = AgentLoop::build(
        Arc::clone(llm),
        Arc::clone(tool_executor),
        Some(HashSet::new()),
    );
    let (synth_agent_tx, synth_agent_rx) =
        mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let mut synth_config = agent_config;
    synth_config.max_iterations = 1;

    let synth_handle = {
        let synth_loop = Arc::clone(&synth_loop);
        tokio::spawn(async move {
            synth_loop
                .run(synth_messages, synth_config, synth_agent_tx)
                .await
        })
    };

    // Bridge synthesis events — map TextDelta → DebateSynthesisTextDelta.
    let synth_content =
        bridge_synthesis_events(node_id, synth_agent_rx, council_tx).await;

    let _ = synth_handle.await;

    let content = synth_content.unwrap_or_default();

    let _ = council_tx
        .send(CouncilEvent::DebateSynthesisComplete {
            node_id: node_id.to_owned(),
            final_text: content.clone(),
        })
        .await;

    content
}

/// Bridge synthesis-phase events, forwarding text deltas and errors.
async fn bridge_synthesis_events(
    node_id: &str,
    mut rx: mpsc::Receiver<AgentEvent>,
    tx: &mpsc::Sender<CouncilEvent>,
) -> Option<String> {
    let mut content: Option<String> = None;
    let mut has_streamed = false;

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::TextDelta { content: delta } => {
                has_streamed = true;
                let _ = tx
                    .send(CouncilEvent::DebateSynthesisTextDelta {
                        node_id: node_id.to_owned(),
                        delta,
                    })
                    .await;
            }
            AgentEvent::FinalAnswer { content: answer } => {
                content = Some(answer);
            }
            _ => {}
        }
    }

    // Safety net: if FinalAnswer arrived but no TextDelta events were
    // streamed (e.g. non-streaming LLM response), emit the full answer
    // as a single delta so the user sees it.
    if !has_streamed {
        if let Some(ref answer) = content {
            if !answer.is_empty() {
                let _ = tx
                    .send(CouncilEvent::DebateSynthesisTextDelta {
                        node_id: node_id.to_owned(),
                        delta: answer.clone(),
                    })
                    .await;
            }
        }
    }

    content
}
