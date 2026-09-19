//! The proxy arm's part of the agentic report: what the eval measured with
//! every turn sent through a real `gglib-proxy`, beside the raw arm it is
//! paired with.
//!
//! # Why a pair of its own
//!
//! The raw and gglib arms open a task that demands a tool call with
//! `tool_choice: "required"`. The proxy judges a call only on a turn left to
//! the model (`tool_choice` `"auto"` or absent), or on one gglib's own grammar
//! constrained, which it installs when a dialect model is asked for
//! `"required"`. Any other `"required"` turn it forwards unjudged. So the proxy
//! arm opens with `"auto"`, where the proxy judges every call whose schema it
//! can judge, whatever the model's tool-call format, and so does the arm it is compared with,
//! [`EvalArm::RawAuto`]: the same requests, sent straight to llama-server.
//!
//! The delta between them is everything the proxy does to those requests and
//! their answers: its request pipeline (per-model sampling among it), repair,
//! the loop guard and response normalisation, together. It does not say which
//! of them moved a score. The repair counts in [`ProxyArms::defects`] say
//! whether repair did anything at all.
//!
//! Neither is compared with the raw or gglib arm. Each of those pairs changes
//! the opening `tool_choice` as well as the machinery, and a delta across two
//! changes attributes to neither.

use serde::{Deserialize, Serialize};

use super::super::tune::config::ScoreWeights;
use super::super::tune::result::TuneTaskResult;
// Named only by the doc links below.
#[cfg(doc)]
use super::EvalArm;
use super::{AgenticEvalReport, ArmDelta, ArmScores, PairedEffect};
use crate::domain::defect_counts::ModelDefectCounts;
use crate::settings::LoopGuardMode;

/// The proxy arm and its raw-auto pair, when the eval ran them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ProxyArms {
    /// Scores under [`EvalArm::RawAuto`].
    pub raw_auto: ArmScores,
    /// Scores under [`EvalArm::Proxy`].
    pub proxy: ArmScores,
    /// Per-axis `proxy − raw_auto` differences.
    ///
    /// Computed by [`AgenticEvalReport::delta_of`], whose field names are the
    /// raw-versus-gglib pair's: when it is withheld, its `raw` count is
    /// raw-auto's unmeasured runs and its `gglib` count the proxy arm's.
    pub delta: ArmDelta,
    /// The paired per-`(task, seed)` comparison, `proxy − raw_auto`. `None`
    /// when no pair had both sides measured.
    pub paired: Option<PairedEffect>,
    /// What the in-process proxy counted over the proxy arm: requests, repairs
    /// attempted and succeeded, loop-guard interventions, and the rest.
    ///
    /// The proxy is started just before this arm and stopped just after it,
    /// and no other arm sends it a request, so these are the arm's own totals.
    /// They say whether repair did anything at all, which the scores cannot: a
    /// repaired call reaches the agent as the repaired call.
    ///
    /// With repair on, zero repairs attempted means no call the proxy could
    /// judge broke its schema. A call whose schema it cannot judge is forwarded without a
    /// count here.
    pub defects: ModelDefectCounts,
    /// Two of the settings the proxy ran under. The others it was given are
    /// fixed and not recorded: client sampling trusted, so each run's seed
    /// reaches llama-server, and no global sampling defaults.
    pub settings: ProxyArmSettings,
    /// Per-task drill-down, one entry per suite task, in suite order.
    pub tasks: Vec<ProxyTaskRuns>,
}

impl ProxyArms {
    /// Assemble the pair's part of the report from its two scored arms,
    /// its per-task runs and what the proxy counted.
    ///
    /// The delta and the paired comparison are computed here, once, the
    /// same way the raw-versus-gglib ones are: [`AgenticEvalReport::delta_of`]
    /// and [`PairedEffect::from_seed_pairs`], with raw-auto as the baseline.
    #[must_use]
    pub fn assemble(
        raw_auto: ArmScores,
        proxy: ArmScores,
        weights: &ScoreWeights,
        defects: ModelDefectCounts,
        settings: ProxyArmSettings,
        tasks: Vec<ProxyTaskRuns>,
    ) -> Self {
        let delta = AgenticEvalReport::delta_of(&raw_auto, &proxy, weights);
        let paired = PairedEffect::from_seed_pairs(
            tasks
                .iter()
                .flat_map(|task| task.raw_auto.iter().zip(task.proxy.iter())),
        );
        Self {
            raw_auto,
            proxy,
            delta,
            paired,
            defects,
            settings,
            tasks,
        }
    }
}

/// Two of the settings the in-process proxy ran under.
///
/// The eval fixes the proxy's settings rather than reading this machine's, so
/// a reading means the same on any machine. These two are recorded because
/// they decide what the pair can show. The rest are fixed and listed at
/// [`ProxyArms::settings`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ProxyArmSettings {
    /// Whether tool-call repair could run, as read when the arm's proxy
    /// started. `false` only when `GGLIB_DISABLE_TOOL_REPAIR` was set in the
    /// environment of the process running the eval, which switches repair off
    /// in every proxy that process starts. The proxy reads it again on every
    /// turn, so a change during the arm is not recorded.
    pub tool_call_repair: bool,
    /// What the loop guard did with a tripped conversation.
    pub loop_guard_mode: LoopGuardMode,
}

/// One task's per-seed runs under the two arms, in seed order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ProxyTaskRuns {
    /// Task identifier from the suite.
    pub task_id: String,
    /// Per-seed results under [`EvalArm::RawAuto`].
    pub raw_auto: Vec<TuneTaskResult>,
    /// Per-seed results under [`EvalArm::Proxy`].
    pub proxy: Vec<TuneTaskResult>,
}

#[cfg(test)]
#[path = "agentic_proxy_tests.rs"]
mod agentic_proxy_tests;
