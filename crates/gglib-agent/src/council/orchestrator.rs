//! Round×agent orchestration loop for a council deliberation.
//!
//! ```text
//! CouncilOrchestrator::run()
//!   │
//!   ├─ for each round 0..N
//!   │   ├─ emit RoundSeparator (round > 0)
//!   │   └─ for each agent
//!   │       ├─ emit AgentTurnStart
//!   │       ├─ build_agent_messages()          (history.rs)
//!   │       ├─ AgentLoop::run()                (delegated, stagnation+loop guards active)
//!   │       ├─ bridge_agent_events()           (stream_bridge.rs)
//!   │       ├─ emit AgentTurnComplete          (with core claim extraction)
//!   │       └─ state.push(contribution)
//!   │
//!   ├─ emit SynthesisStart
//!   ├─ AgentLoop::run() with synthesis prompt
//!   ├─ bridge synthesis events
//!   ├─ emit SynthesisComplete
//!   └─ emit CouncilComplete
//! ```

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, LlmCompletionPort,
    ToolExecutorPort,
};

use crate::AgentLoop;

use super::config::CouncilConfig;
use super::events::CouncilEvent;
use super::history::{build_agent_messages, format_synthesis_transcript};
use super::prompts::SYNTHESIS_PROMPT;
use super::state::{AgentContribution, CouncilState, extract_core_claim};
use super::stream_bridge::{bridge_agent_events, emit_turn_complete};

/// Runs a full council deliberation: rounds → agent turns → synthesis.
///
/// This function is the only public entry point.  It owns the round×agent
/// loop and delegates each agent turn to an [`AgentLoop`] via the existing
/// port traits.  The caller provides a `council_tx` to receive streamed
/// [`CouncilEvent`]s.
///
/// # Errors
///
/// Individual agent errors (stagnation, loop detection, max iterations) are
/// handled gracefully — the contribution is recorded as-is and the council
/// proceeds.  Only infrastructure-level failures (channel closure) cause an
/// early return.
pub async fn run(
    config: CouncilConfig,
    agent_config: AgentConfig,
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    council_tx: mpsc::Sender<CouncilEvent>,
) {
    let mut state = CouncilState::new();

    // ── debate rounds ────────────────────────────────────────────────────
    for round in 0..config.rounds {
        if round > 0 {
            if send(&council_tx, CouncilEvent::RoundSeparator { round })
                .await
                .is_err()
            {
                return;
            }
        }

        for agent in &config.agents {
            // Announce the turn.
            let start = CouncilEvent::AgentTurnStart {
                agent_id: agent.id.clone(),
                agent_name: agent.name.clone(),
                color: agent.color.clone(),
                round,
                contentiousness: agent.contentiousness,
            };
            if send(&council_tx, start).await.is_err() {
                return;
            }

            // Build per-agent tool filter.
            let filter = agent
                .tool_filter
                .as_ref()
                .map(|names| names.iter().cloned().collect::<HashSet<String>>());

            // Build the agent loop with per-agent tool restrictions.
            let agent_loop = AgentLoop::build(Arc::clone(&llm), Arc::clone(&tool_executor), filter);

            // Assemble context with identity anchoring + debate transcript.
            let messages = build_agent_messages(agent, &config.topic, round, config.rounds, &state);

            // Delegate to AgentLoop — stagnation + loop guards are active
            // via agent_config settings (max_stagnation_steps, max_repeated_batch_steps).
            let (agent_tx, agent_rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
            let loop_handle = {
                let agent_loop = Arc::clone(&agent_loop);
                let cfg = agent_config.clone();
                tokio::spawn(async move { agent_loop.run(messages, cfg, agent_tx).await })
            };

            // Bridge events from agent → council stream.
            let answer = bridge_agent_events(agent, round, agent_rx, &council_tx).await;

            // Await the agent loop completion (the channel is already drained).
            match loop_handle.await {
                Ok(Ok(_output)) => {
                    debug!(agent_id = %agent.id, round, "agent turn completed normally");
                }
                Ok(Err(e)) => {
                    // Stagnation, loop detection, max iterations — all graceful.
                    warn!(agent_id = %agent.id, round, error = %e, "agent turn ended early");
                }
                Err(e) => {
                    warn!(agent_id = %agent.id, round, error = %e, "agent task panicked");
                }
            }

            // Record the contribution (use whatever content we got).
            let content = answer.unwrap_or_default();
            let core_claim = extract_core_claim(&content);
            emit_turn_complete(agent, round, &content, &council_tx).await;

            state.push(AgentContribution {
                agent: agent.clone(),
                content,
                core_claim,
                round,
            });
        }

        state.advance_round();
    }

    // ── synthesis ────────────────────────────────────────────────────────
    run_synthesis(&config, agent_config, &llm, &tool_executor, &state, &council_tx).await;
}

/// Synthesis pass: build the transcript, run a single-iteration agent loop,
/// and emit `SynthesisStart` / `SynthesisTextDelta` / `SynthesisComplete` /
/// `CouncilComplete`.
async fn run_synthesis(
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
