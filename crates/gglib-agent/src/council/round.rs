//! Sequential round execution for a council deliberation.
//!
//! Each round iterates over agents in declaration order, delegating each
//! turn to an [`AgentLoop`](crate::AgentLoop) and recording the resulting
//! [`AgentContribution`] in the shared [`CouncilState`].
//!
//! This module owns:
//! - per-agent tool filter construction
//! - agent loop spawning + error handling
//! - contribution recording (content + core claim extraction)

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, warn};

use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, LlmCompletionPort, ToolExecutorPort,
};

use crate::AgentLoop;

use super::config::{CouncilAgent, CouncilConfig};
use super::events::CouncilEvent;
use super::history::build_agent_messages;
use super::state::{AgentContribution, CouncilState, extract_core_claim};
use super::stream_bridge::{bridge_agent_events, emit_turn_complete};

/// Shared, immutable context threaded through every agent turn in a round.
///
/// Bundles the dependencies that every turn needs so individual functions
/// stay below the clippy `too_many_arguments` threshold.
pub(super) struct RoundContext<'a> {
    pub config: &'a CouncilConfig,
    pub agent_config: &'a AgentConfig,
    pub llm: &'a Arc<dyn LlmCompletionPort>,
    pub tool_executor: &'a Arc<dyn ToolExecutorPort>,
    pub council_tx: &'a mpsc::Sender<CouncilEvent>,
}

/// Execute a single debate round sequentially: each agent speaks in
/// declaration order, receiving the full debate transcript up to this point.
///
/// Returns `Err(())` if the council channel is closed (caller should stop).
pub(super) async fn run_sequential_round(
    round: u32,
    ctx: &RoundContext<'_>,
    state: &mut CouncilState,
) -> Result<(), ()> {
    for agent in &ctx.config.agents {
        run_agent_turn(agent, round, ctx, state).await?;
    }
    Ok(())
}

/// Execute a single agent's turn within a round.
///
/// Steps:
/// 1. Emit `AgentTurnStart`
/// 2. Build identity-anchored messages with debate transcript
/// 3. Spawn `AgentLoop::run()` with per-agent tool filter
/// 4. Bridge events from agent → council stream
/// 5. Record contribution (content + core claim)
///
/// Returns `Err(())` if the council channel is closed.
async fn run_agent_turn(
    agent: &CouncilAgent,
    round: u32,
    ctx: &RoundContext<'_>,
    state: &mut CouncilState,
) -> Result<(), ()> {
    // Announce the turn.
    let start = CouncilEvent::AgentTurnStart {
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        color: agent.color.clone(),
        round,
        contentiousness: agent.contentiousness,
    };
    if ctx.council_tx.send(start).await.is_err() {
        return Err(());
    }

    // Build per-agent tool filter.
    let filter = agent
        .tool_filter
        .as_ref()
        .map(|names| names.iter().cloned().collect::<HashSet<String>>());

    // Build the agent loop with per-agent tool restrictions.
    let agent_loop = AgentLoop::build(Arc::clone(ctx.llm), Arc::clone(ctx.tool_executor), filter);

    // Assemble context with identity anchoring + debate transcript.
    let messages = build_agent_messages(agent, &ctx.config.topic, round, ctx.config.rounds, state);

    // Delegate to AgentLoop — stagnation + loop guards are active
    // via agent_config settings (max_stagnation_steps, max_repeated_batch_steps).
    let (agent_tx, agent_rx) = mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);
    let loop_handle = {
        let agent_loop = Arc::clone(&agent_loop);
        let cfg = ctx.agent_config.clone();
        tokio::spawn(async move { agent_loop.run(messages, cfg, agent_tx).await })
    };

    // Bridge events from agent → council stream.
    let answer = bridge_agent_events(agent, round, agent_rx, ctx.council_tx).await;

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
    emit_turn_complete(agent, round, &content, ctx.council_tx).await;

    state.push(AgentContribution {
        agent: agent.clone(),
        content,
        core_claim,
        round,
    });

    Ok(())
}
