//! Synthesis pass for a council deliberation.
//!
//! After all debate rounds complete, the synthesiser builds a full debate
//! transcript and runs a single-iteration [`AgentLoop`](crate::AgentLoop)
//! to produce a unified answer that integrates all agent positions.
//!
//! This module owns:
//! - transcript → synthesis prompt assembly
//! - synthesis event bridging (`TextDelta` → `SynthesisTextDelta`)
//! - `SynthesisComplete` / `CouncilComplete` emission

use std::sync::Arc;

use tokio::sync::mpsc;

use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, LlmCompletionPort,
    ToolExecutorPort,
};

use crate::AgentLoop;

use super::config::CouncilConfig;
use super::events::CouncilEvent;
use super::history::format_synthesis_transcript;
use super::prompts::SYNTHESIS_PROMPT;
use super::state::CouncilState;

/// Run the synthesis phase: build the transcript, call a single-iteration
/// [`AgentLoop`], and emit `SynthesisStart` / `SynthesisTextDelta` /
/// `SynthesisComplete` / `CouncilComplete`.
///
/// The synthesis agent has no tool restrictions and runs for exactly one
/// iteration — it produces a prose answer, never tool calls.
pub(super) async fn run_synthesis(
    config: &CouncilConfig,
    agent_config: AgentConfig,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    state: &CouncilState,
    council_tx: &mpsc::Sender<CouncilEvent>,
) {
    if send(council_tx, CouncilEvent::SynthesisStart)
        .await
        .is_err()
    {
        return;
    }

    let transcript = format_synthesis_transcript(state);
    let guidance = config
        .synthesis_guidance
        .as_deref()
        .unwrap_or("Provide an actionable synthesis.");

    #[allow(clippy::literal_string_with_formatting_args)]
    let synthesis_prompt = SYNTHESIS_PROMPT
        .replace("{agent_count}", &config.agents.len().to_string())
        .replace("{topic}", &config.topic)
        .replace("{transcript}", &transcript)
        .replace("{synthesis_guidance}", guidance);

    let synth_messages = vec![
        AgentMessage::System {
            content: synthesis_prompt,
        },
        AgentMessage::User {
            content: config.topic.clone(),
        },
    ];

    let synth_loop = AgentLoop::build(Arc::clone(llm), Arc::clone(tool_executor), None);
    let (synth_agent_tx, synth_agent_rx) =
        mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    // Synthesis uses a restricted config — no tools needed, single iteration.
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

    // Bridge synthesis events — map TextDelta → SynthesisTextDelta.
    let synth_content = bridge_synthesis_events(synth_agent_rx, council_tx).await;

    let _ = synth_handle.await;

    let content = synth_content.unwrap_or_default();
    let _ = send(council_tx, CouncilEvent::SynthesisComplete { content }).await;
    let _ = send(council_tx, CouncilEvent::CouncilComplete).await;
}

/// Bridge synthesis-phase events (only text deltas are relevant).
async fn bridge_synthesis_events(
    mut rx: mpsc::Receiver<AgentEvent>,
    tx: &mpsc::Sender<CouncilEvent>,
) -> Option<String> {
    let mut content: Option<String> = None;
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::TextDelta { content: delta } => {
                let _ = tx.send(CouncilEvent::SynthesisTextDelta { delta }).await;
            }
            AgentEvent::FinalAnswer { content: answer } => {
                content = Some(answer);
            }
            _ => {}
        }
    }
    content
}

/// Best-effort send helper — returns `Err` when the receiver is gone.
async fn send(
    tx: &mpsc::Sender<CouncilEvent>,
    event: CouncilEvent,
) -> Result<(), mpsc::error::SendError<CouncilEvent>> {
    tx.send(event).await
}
