//! Sequential debate round execution.
//!
//! Runs all agents in order for a single round, checking the cancellation
//! token between every agent turn to allow prompt abort on GPU-constrained
//! hardware.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::task_graph::{DebateAgent, DebateConfig};
use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig};

use crate::AgentLoop;

use super::history::build_agent_messages;
use super::state::{AgentContribution, DebateState, extract_core_claim};
use super::stream_bridge::{bridge_agent_events, emit_turn_complete};

/// Context shared across all turns within a round.
pub(super) struct RoundContext<'a> {
    pub config: &'a DebateConfig,
    pub agent_config: &'a AgentConfig,
    pub llm: &'a Arc<dyn LlmCompletionPort>,
    pub tool_executor: &'a Arc<dyn ToolExecutorPort>,
    pub council_tx: &'a mpsc::Sender<CouncilEvent>,
    pub cancel: CancellationToken,
}

/// Run all agents in order for a single debate round.
///
/// Checks the cancellation token after every agent turn.  Returns `Err(())`
/// if cancelled or if any agent turn hard-fails.
pub(super) async fn run_sequential_round(
    node_id: &str,
    topic: &str,
    round: u32,
    ctx: &RoundContext<'_>,
    state: &mut DebateState,
) -> Result<(), ()> {
    for agent in &ctx.config.agents {
        // Check cancellation before starting each agent.
        if ctx.cancel.is_cancelled() {
            return Err(());
        }

        run_agent_turn(node_id, topic, round, agent, ctx, state).await?;

        // Check cancellation after each agent turn.
        if ctx.cancel.is_cancelled() {
            return Err(());
        }
    }
    Ok(())
}

/// Run a single agent's turn within a round.
async fn run_agent_turn(
    node_id: &str,
    topic: &str,
    round: u32,
    agent: &DebateAgent,
    ctx: &RoundContext<'_>,
    state: &mut DebateState,
) -> Result<(), ()> {
    // Build the tool filter.
    let tool_filter: Option<HashSet<String>> = agent
        .tool_filter
        .as_ref()
        .map(|f| std::iter::once(f.clone()).collect());

    // Announce turn start.
    let _ = ctx
        .council_tx
        .send(CouncilEvent::DebateAgentTurnStarted {
            node_id: node_id.to_owned(),
            agent_id: agent.id.clone(),
            agent_name: agent.name.clone(),
            color: agent.color.clone(),
            round: round + 1, // 1-based
            contentiousness: agent.contentiousness,
        })
        .await;

    // Build messages.
    let messages = build_agent_messages(agent, topic, round, ctx.config.rounds, state);

    // Build agent loop.
    let agent_loop = AgentLoop::build(
        Arc::clone(ctx.llm),
        Arc::clone(ctx.tool_executor),
        tool_filter,
    );

    let (agent_tx, agent_rx) =
        mpsc::channel::<gglib_core::AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let config = ctx.agent_config.clone();
    let handle = {
        let agent_loop = Arc::clone(&agent_loop);
        tokio::spawn(async move { agent_loop.run(messages, config, agent_tx).await })
    };

    // Bridge events.
    let final_content = bridge_agent_events(node_id, agent, round, agent_rx, ctx.council_tx).await;

    let _ = handle.await;

    let content = final_content.unwrap_or_default();

    // Emit turn complete.
    emit_turn_complete(node_id, agent, round, &content, ctx.council_tx).await;

    // Record contribution.
    let core_claim = extract_core_claim(&content);
    state.push(AgentContribution {
        agent: agent.clone(),
        content,
        core_claim,
        round,
    });

    Ok(())
}
