//! Top-level coordinator for a council deliberation.
//!
//! This module is intentionally slim — it sequences the high-level phases
//! (debate rounds → compaction → optional judge → synthesis) and delegates
//! all per-agent and per-phase logic to dedicated sub-modules:
//!
//! ```text
//! orchestrator::run()
//!   │
//!   ├─ for each round 0..N
//!   │   ├─ emit RoundSeparator (round > 0)
//!   │   ├─ round::run_sequential_round()      (round.rs)
//!   │   ├─ compaction::compact_round()         (compaction.rs)
//!   │   └─ if judge enabled:
//!   │       └─ judge::run_judge()              (judge.rs)
//!   │           └─ if consensus && may_stop → break
//!   │
//!   ├─ stance::evaluate_stances()              (stance.rs)
//!   └─ synthesis::run_synthesis()              (synthesis.rs)
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::info;

use gglib_core::{AgentConfig, LlmCompletionPort, ToolExecutorPort};

use super::compaction::compact_round;
use super::config::CouncilConfig;
use super::events::CouncilEvent;
use super::judge::{may_stop_early, run_judge};
use super::round::{RoundContext, run_sequential_round};
use super::stance::evaluate_stances;
use super::state::CouncilState;
use super::synthesis::run_synthesis;

/// Runs a full council deliberation: debate rounds → compaction → optional judge → stance evaluation → synthesis.
///
/// This function is the only public entry point.  It coordinates the
/// high-level phase sequence and delegates per-agent turn execution to
/// [`round::run_sequential_round`], round compaction to
/// [`compaction::compact_round`], optional judge evaluation to
/// [`judge::run_judge`], and the synthesis pass to
/// [`synthesis::run_synthesis`].
///
/// # Round Compaction
///
/// After each round completes (except the most recent), the orchestrator
/// runs a lightweight compaction pass that summarises the round's
/// contributions into a short per-agent summary.  Subsequent agents see
/// the compacted text instead of the full transcript, keeping context
/// sizes manageable in long debates.
///
/// # Judge + Adaptive Early Stopping
///
/// When `config.judge` is `Some`, a neutral judge evaluates the debate
/// after each round.  If the judge determines consensus has been reached
/// and the minimum-rounds threshold is met, remaining rounds are skipped.
///
/// # Errors
///
/// Individual agent errors (stagnation, loop detection, max iterations) are
/// handled gracefully inside `round.rs` — the contribution is recorded
/// as-is and the council proceeds.  Only infrastructure-level failures
/// (channel closure) cause an early return.
pub async fn run(
    config: CouncilConfig,
    agent_config: AgentConfig,
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    council_tx: mpsc::Sender<CouncilEvent>,
    cwd: Option<PathBuf>,
) {
    let mut state = CouncilState::new();

    let ctx = RoundContext {
        config: &config,
        agent_config: &agent_config,
        llm: &llm,
        tool_executor: &tool_executor,
        council_tx: &council_tx,
        cwd: cwd.as_deref(),
    };

    // ── debate rounds ────────────────────────────────────────────────────
    for round in 0..config.rounds {
        if round > 0 {
            if council_tx
                .send(CouncilEvent::RoundSeparator { round })
                .await
                .is_err()
            {
                return;
            }
        }

        if run_sequential_round(round, &ctx, &mut state).await.is_err() {
            return;
        }

        state.advance_round();

        // ── compaction ───────────────────────────────────────────────────
        // Summarise the just-completed round so that future agents see a
        // compact version rather than the full transcript.
        compact_round(
            round,
            &mut state,
            &llm,
            &tool_executor,
            &council_tx,
            &config.topic,
        )
        .await;

        // ── optional judge evaluation ────────────────────────────────────
        if let Some(ref judge_config) = config.judge {
            let completed_rounds = round + 1;
            let is_last_round = completed_rounds >= config.rounds;

            // Skip judge on the final round — synthesis follows regardless.
            if !is_last_round {
                if let Some(verdict) = run_judge(
                    round,
                    config.rounds,
                    judge_config,
                    &llm,
                    &tool_executor,
                    &state,
                    &council_tx,
                    &config.topic,
                )
                .await
                {
                    if verdict.consensus_reached && may_stop_early(judge_config, completed_rounds) {
                        info!(
                            round,
                            completed_rounds, "judge detected consensus — stopping early"
                        );
                        break;
                    }
                }
            }
        }
    }

    // ── stance evaluation ────────────────────────────────────────────────
    evaluate_stances(&state, &llm, &tool_executor, &council_tx, &config.topic).await;

    // ── synthesis ────────────────────────────────────────────────────────
    run_synthesis(
        &config,
        agent_config,
        &llm,
        &tool_executor,
        &state,
        &council_tx,
    )
    .await;
}
