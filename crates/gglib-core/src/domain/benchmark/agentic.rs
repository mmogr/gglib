//! Raw-vs-gglib A/B agentic evaluation: config and report types.
//!
//! The eval answers one question with numbers: *what does routing a small
//! model through the gglib pipeline actually buy in agentic behaviour?* It
//! runs the same task suite the tune sweep uses — real `AgentLoop`, scripted
//! BFCL-style tasks — twice against the same loaded model:
//!
//! - **raw**: the request pipeline bypassed entirely. No sampling
//!   resolution (the server's own defaults apply), no capability shaping,
//!   no dialect normalization, no grammar — what a client pointed straight
//!   at llama-server experiences.
//! - **gglib**: the full pipeline, exactly as the proxy runs it — per-model
//!   sampling defaults, capability-aware shaping, dialect parsing, and
//!   decode-time grammar enforcement where a task demands a tool call.
//!
//! The per-axis deltas in the [`AgenticEvalReport`] are the product: the
//! measured difference in tool-call accuracy, loop avoidance, and task
//! completion, on this model, on this machine.

use serde::{Deserialize, Serialize};

use super::tune::config::ScoreWeights;
use super::tune::result::TuneTaskResult;
use super::tune::task::{TaskCategory, TaskSuite};

/// Configuration for one A/B agentic eval run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticEvalConfig {
    /// Database ID of the model to evaluate.
    pub model_id: i64,
    /// Task suite both arms run — the same schema the tune sweep uses.
    pub task_suite: TaskSuite,
    /// Weights for each arm's composite score.
    #[serde(default)]
    pub weights: ScoreWeights,
    /// Context size override (tokens). `None` resolves through the normal
    /// chain (model server defaults → global setting → hardcoded default).
    #[serde(default)]
    pub ctx_size: Option<u64>,
}

/// Which arm a task ran under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalArm {
    /// Pipeline bypassed — bare llama-server behaviour.
    Raw,
    /// The full gglib request/response pipeline.
    Gglib,
}

impl std::fmt::Display for EvalArm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "raw"),
            Self::Gglib => write!(f, "gglib"),
        }
    }
}

/// One arm's aggregate scores across the task suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmScores {
    /// Mean AST-style tool-call match score, `0.0`–`1.0`.
    pub tool_accuracy: f64,
    /// Fraction of *loop-eligible* tasks that triggered neither the loop nor
    /// the stagnation guard.
    ///
    /// `None` when no task in this arm ever reached a second tool-call batch,
    /// so the guards had nothing to fire on: the axis was not measured, which
    /// is distinct from a perfect `1.0`. Read it together with
    /// [`Self::loop_eligible`], which is its denominator.
    #[serde(default)]
    pub loop_avoidance: Option<f64>,
    /// How many of this arm's tasks were loop-eligible — the sample size
    /// behind [`Self::loop_avoidance`].
    #[serde(default)]
    pub loop_eligible: usize,
    /// Fraction of tasks passed outright.
    pub task_completion: f64,
    /// Weighted composite of the axes above, over whichever of them were
    /// measured. An unmeasured loop-avoidance axis claims no weight rather
    /// than scoring zero.
    pub composite: f64,
    /// Completion-token throughput (tokens per wall-clock second, pre-fill
    /// included). `None` when the upstream reported no usage.
    pub tg_tps: Option<f64>,
    /// Total completion tokens generated across the whole suite. `None` when
    /// no task reported usage.
    ///
    /// Reported beside the composite and never folded into it: token cost is
    /// what the quality axes cannot see, but it is also hardware- and
    /// model-specific in a way that would make a single blended score
    /// incomparable across machines.
    #[serde(default)]
    pub total_completion_tokens: Option<u64>,
    /// Total wall-clock milliseconds across every task in the suite,
    /// unfiltered — the honest cost of running it.
    #[serde(default)]
    pub total_wall_ms: u64,
    /// Mean time to the first tool call, over the tasks that made one. `None`
    /// when no task in the arm called a tool.
    #[serde(default)]
    pub mean_time_to_first_tool_call_ms: Option<f64>,
}

/// Per-axis difference, `gglib − raw`. Positive means gglib scored higher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArmDelta {
    /// Tool-accuracy difference.
    pub tool_accuracy: f64,
    /// Loop-avoidance difference.
    ///
    /// `None` unless *both* arms measured the axis — a difference against an
    /// arm that never risked a loop would be arithmetic on a number that was
    /// never observed.
    #[serde(default)]
    pub loop_avoidance: Option<f64>,
    /// Task-completion difference.
    pub task_completion: f64,
    /// Composite-score difference.
    pub composite: f64,
    /// Suite wall-time speedup, `raw ÷ gglib`. Above `1.0` means gglib
    /// finished the same suite faster.
    ///
    /// A ratio rather than a difference, for two reasons: lower is better
    /// here, so a plain subtraction would invert this struct's "positive means
    /// gglib did better" convention; and the magnitudes are multiplicative —
    /// a 230× gap reads as `230.0`, not as `-1099737` milliseconds. `None`
    /// when the gglib arm recorded no wall time to divide by.
    #[serde(default)]
    pub wall_time_speedup: Option<f64>,
    /// Completion-token ratio, `raw ÷ gglib`. Above `1.0` means gglib reached
    /// the same outcome on fewer generated tokens. `None` when either arm went
    /// unmeasured or the gglib arm generated nothing.
    #[serde(default)]
    pub completion_token_ratio: Option<f64>,
}

/// One task's outcome under both arms, for drill-down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticTaskComparison {
    /// Task identifier from the suite.
    pub task_id: String,
    /// The task's BFCL-style category.
    pub category: TaskCategory,
    /// Result under the raw arm.
    pub raw: TuneTaskResult,
    /// Result under the gglib arm.
    pub gglib: TuneTaskResult,
}

/// The complete A/B report — the leaderboard interchange format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticEvalReport {
    /// Model name as stored in the catalog.
    pub model_name: String,
    /// Quantization label (e.g. `Q4_K_M`), when known.
    pub quantization: Option<String>,
    /// Parameter count in billions.
    pub param_count_b: f64,
    /// Context size both arms ran at, in tokens.
    pub ctx_size: u64,
    /// Aggregate scores under the raw arm.
    pub raw: ArmScores,
    /// Aggregate scores under the gglib arm.
    pub gglib: ArmScores,
    /// Per-axis `gglib − raw` differences.
    pub delta: ArmDelta,
    /// Per-task drill-down, one entry per suite task.
    pub tasks: Vec<AgenticTaskComparison>,
}

impl AgenticEvalReport {
    /// Compute the per-axis delta from the two arms' scores.
    #[must_use]
    pub fn delta_of(raw: &ArmScores, gglib: &ArmScores) -> ArmDelta {
        ArmDelta {
            tool_accuracy: gglib.tool_accuracy - raw.tool_accuracy,
            loop_avoidance: gglib
                .loop_avoidance
                .zip(raw.loop_avoidance)
                .map(|(g, r)| g - r),
            task_completion: gglib.task_completion - raw.task_completion,
            composite: gglib.composite - raw.composite,
            wall_time_speedup: ratio(
                Some(as_f64(raw.total_wall_ms)),
                Some(as_f64(gglib.total_wall_ms)),
            ),
            completion_token_ratio: ratio(
                raw.total_completion_tokens.map(as_f64),
                gglib.total_completion_tokens.map(as_f64),
            ),
        }
    }
}

/// `raw ÷ gglib`, or `None` when either side is unmeasured or the denominator
/// is zero — an infinite speedup is not a measurement.
fn ratio(raw: Option<f64>, gglib: Option<f64>) -> Option<f64> {
    match (raw, gglib) {
        (Some(r), Some(g)) if g > 0.0 => Some(r / g),
        _ => None,
    }
}

/// Widen a count for ratio arithmetic. Suite totals are far below the 2^53
/// boundary where `f64` stops representing integers exactly.
#[allow(clippy::cast_precision_loss)]
const fn as_f64(value: u64) -> f64 {
    value as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scores(tool_accuracy: f64, loop_avoidance: Option<f64>, composite: f64) -> ArmScores {
        ArmScores {
            tool_accuracy,
            loop_avoidance,
            loop_eligible: usize::from(loop_avoidance.is_some()),
            task_completion: 0.25,
            composite,
            tg_tps: Some(30.0),
            total_completion_tokens: Some(1_000),
            total_wall_ms: 1_000,
            mean_time_to_first_tool_call_ms: Some(100.0),
        }
    }

    #[test]
    fn delta_is_gglib_minus_raw() {
        let mut raw = scores(0.5, Some(0.75), 0.5);
        raw.task_completion = 0.25;
        let mut gglib = scores(0.9, Some(1.0), 0.9);
        gglib.task_completion = 0.75;

        let delta = AgenticEvalReport::delta_of(&raw, &gglib);
        assert!((delta.tool_accuracy - 0.4).abs() < 1e-9);
        assert!((delta.loop_avoidance.unwrap() - 0.25).abs() < 1e-9);
        assert!((delta.task_completion - 0.5).abs() < 1e-9);
        assert!((delta.composite - 0.4).abs() < 1e-9);
    }

    /// The efficiency rows are ratios, not differences: lower is better on
    /// both, so a subtraction would invert the struct's "positive means gglib
    /// did better" convention. These are the figures from the run that
    /// motivated the fix.
    #[test]
    fn efficiency_factors_are_raw_over_gglib() {
        let mut raw = scores(0.722, Some(1.0), 0.802);
        raw.total_wall_ms = 1_104_543;
        raw.total_completion_tokens = Some(226_768);
        let mut gglib = scores(0.722, Some(1.0), 0.802);
        gglib.total_wall_ms = 4_806;
        gglib.total_completion_tokens = Some(49);

        let delta = AgenticEvalReport::delta_of(&raw, &gglib);
        assert!((delta.wall_time_speedup.unwrap() - 229.83).abs() < 0.01);
        assert!((delta.completion_token_ratio.unwrap() - 4_627.92).abs() < 0.01);
    }

    /// An infinite speedup is not a measurement.
    #[test]
    fn a_zero_denominator_yields_no_factor() {
        let raw = scores(0.5, Some(1.0), 0.5);
        let mut gglib = scores(0.5, Some(1.0), 0.5);
        gglib.total_wall_ms = 0;
        gglib.total_completion_tokens = Some(0);

        let delta = AgenticEvalReport::delta_of(&raw, &gglib);
        assert!(delta.wall_time_speedup.is_none());
        assert!(delta.completion_token_ratio.is_none());
    }

    /// Subtracting against an arm that never risked a loop would be arithmetic
    /// on a figure nobody observed — exactly the comparison that reported a
    /// bare llama-server arm as beating the pipeline.
    #[test]
    fn an_unmeasured_arm_yields_no_loop_avoidance_delta() {
        let raw = scores(0.5, None, 0.5);
        let gglib = scores(0.9, Some(0.0), 0.9);
        assert!(
            AgenticEvalReport::delta_of(&raw, &gglib)
                .loop_avoidance
                .is_none()
        );
        assert!(
            AgenticEvalReport::delta_of(&gglib, &raw)
                .loop_avoidance
                .is_none()
        );
    }

    #[test]
    fn config_round_trips_with_defaults() {
        let json = r#"{"model_id": 3, "task_suite": {"source": "default"}}"#;
        let config: AgenticEvalConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.model_id, 3);
        assert!(config.ctx_size.is_none());
    }
}
