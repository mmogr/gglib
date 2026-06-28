#![doc = include_str!("README.md")]
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use gglib_core::AgentConfig;
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::task_graph::DebateConfig;
use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};

use round::RoundContext;

mod compaction;
mod history;
mod judge;
pub mod prompts;
mod round;
mod stance;
mod state;
mod stream_bridge;
mod synthesis;

// Re-export for executor integration.
pub use state::DebateState;

/// Error type for debate node execution.
#[derive(Debug)]
pub enum DebateError {
    /// The run was cancelled via the cancellation token.
    Cancelled,
    /// An agent turn hard-failed (channel closed or unrecoverable error).
    AgentFailed,
}

/// Run a debate node to completion.
///
/// # Arguments
///
/// - `node_id` — the string id of the `Debate` node in the task graph.
/// - `topic` — the goal text from the node (used as the debate topic).
/// - `predecessor_context` — output from predecessor nodes (currently unused
///   at synthesis level; reserved for future context injection).
/// - `config` — debate configuration from [`TaskNodeKind::Debate`].
/// - `llm` — shared LLM completion port.
/// - `tool_executor` — shared tool executor port.
/// - `agent_config` — base agent config (`max_iterations`, etc.).
/// - `tx` — the council event sender.
/// - `cancel` — cancellation token; checked between every agent turn and
///   after every round to allow prompt abort on GPU-constrained hardware.
///
/// # Returns
///
/// `Ok(synthesis_text)` on success, `Err(DebateError)` on cancellation or
/// hard failure.
#[allow(clippy::too_many_arguments)]
pub async fn run_debate_node(
    node_id: &str,
    topic: &str,
    _predecessor_context: &str,
    config: &DebateConfig,
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    agent_config: &AgentConfig,
    tx: &mpsc::Sender<CouncilEvent>,
    cancel: CancellationToken,
) -> Result<String, DebateError> {
    let mut state = DebateState::new();
    let total_rounds = config.rounds;

    // ── Round loop ────────────────────────────────────────────────────────
    let mut rounds_completed = 0u32;

    for round in 0..total_rounds {
        // Check cancellation at the start of each round.
        if cancel.is_cancelled() {
            return Err(DebateError::Cancelled);
        }

        // Announce round start (1-based in events).
        let _ = tx
            .send(CouncilEvent::DebateRoundStarted {
                node_id: node_id.to_owned(),
                round: round + 1,
            })
            .await;

        // Build the round context.
        let ctx = RoundContext {
            config,
            agent_config,
            llm: &llm,
            tool_executor: &tool_executor,
            council_tx: tx,
            cancel: cancel.clone(),
        };

        // Run all agents sequentially.
        round::run_sequential_round(node_id, topic, round, &ctx, &mut state)
            .await
            .map_err(|()| {
                if cancel.is_cancelled() {
                    DebateError::Cancelled
                } else {
                    DebateError::AgentFailed
                }
            })?;

        rounds_completed += 1;

        // Check cancellation after round completes.
        if cancel.is_cancelled() {
            return Err(DebateError::Cancelled);
        }

        // ── Optional judge (early stopping) ──────────────────────────────
        let should_stop_early = if let Some(judge_cfg) = &config.judge {
            let verdict = judge::run_judge(
                node_id,
                round,
                total_rounds,
                judge_cfg.min_rounds_before_stop,
                topic,
                &llm,
                &tool_executor,
                &state,
                tx,
            )
            .await;

            // `early_stop_recommended` is already encoded in the emitted event;
            // here we use it locally to decide whether to break.
            verdict
                .is_some_and(|v| v.consensus_reached && round >= judge_cfg.min_rounds_before_stop)
        } else {
            false
        };

        // ── Optional compaction (keep context manageable in long debates) ─
        // Only compact when there are enough rounds that it matters.
        if total_rounds > 3 {
            compaction::compact_round(node_id, round, &mut state, &llm, &tool_executor, tx).await;
        }

        // Advance state round pointer.
        state.advance_round();

        if should_stop_early {
            break;
        }
    }

    // ── Post-debate: stance evaluation ────────────────────────────────────
    if cancel.is_cancelled() {
        return Err(DebateError::Cancelled);
    }

    stance::evaluate_stances(node_id, &state, &llm, &tool_executor, tx, topic).await;

    // ── Synthesis ─────────────────────────────────────────────────────────
    if cancel.is_cancelled() {
        return Err(DebateError::Cancelled);
    }

    let synthesis_text = synthesis::run_synthesis(
        node_id,
        topic,
        config,
        agent_config.clone(),
        &llm,
        &tool_executor,
        &state,
        tx,
    )
    .await;

    let _ = rounds_completed; // used for tracing if needed later

    Ok(synthesis_text)
}
