//! Top-level coordinator for a council deliberation.
//!
//! This module is intentionally slim — it sequences the high-level phases
//! (debate rounds → optional judge → synthesis) and delegates all per-agent
//! and per-phase logic to dedicated sub-modules:
//!
//! ```text
//! orchestrator::run()
//!   │
//!   ├─ for each round 0..N
//!   │   ├─ emit RoundSeparator (round > 0)
//!   │   ├─ round::run_sequential_round()      (round.rs)
//!   │   └─ if judge enabled:
//!   │       └─ judge::run_judge()              (judge.rs)
//!   │           └─ if consensus && may_stop → break
//!   │
//!   └─ synthesis::run_synthesis()              (synthesis.rs)
//! ```

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::info;

use gglib_core::{AgentConfig, LlmCompletionPort, ToolExecutorPort};

use super::config::CouncilConfig;
use super::events::CouncilEvent;
use super::judge::{may_stop_early, run_judge};
use super::round::{RoundContext, run_sequential_round};
use super::state::CouncilState;
use super::synthesis::run_synthesis;

/// Runs a full council deliberation: debate rounds → optional judge → synthesis.
///
/// This function is the only public entry point.  It coordinates the
/// high-level phase sequence and delegates per-agent turn execution to
/// [`round::run_sequential_round`], optional judge evaluation to
/// [`judge::run_judge`], and the synthesis pass to
/// [`synthesis::run_synthesis`].
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
) {
    let mut state = CouncilState::new();

    let ctx = RoundContext {
        config: &config,
        agent_config: &agent_config,
        llm: &llm,
        tool_executor: &tool_executor,
        council_tx: &council_tx,
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
                            completed_rounds,
                            "judge detected consensus — stopping early"
                        );
                        break;
                    }
                }
            }
        }
    }

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
