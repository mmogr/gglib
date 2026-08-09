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
    /// RNG seeds to repeat every task under, once each.
    ///
    /// # Why more than one
    ///
    /// A single sample per task is not a measurement of a model, it is one
    /// draw from its output distribution. Two runs of the *identical* raw
    /// configuration have scored `0.728` and `0.543` on this suite — a gap
    /// wider than most of the effects the eval exists to detect. Averaging a
    /// handful of seeds is what separates a real difference from that spread.
    ///
    /// Seeded rather than merely repeated, so a surprising number can be
    /// re-run and reproduced instead of chased. An empty list means one
    /// unseeded run per task, which is the pre-multi-seed behaviour and is
    /// kept reachable deliberately — it is the fastest smoke test.
    #[serde(default = "default_seeds")]
    pub seeds: Vec<u32>,
    /// Whether to run the positive control arm. See [`EvalArm::Control`].
    #[serde(default = "default_include_control")]
    pub include_control: bool,
}

/// The seeds an eval uses when its config names none.
///
/// Three, because it is the smallest count that can distinguish "these two
/// arms differ" from "one of them had an unlucky draw", and each extra seed
/// costs a full pass over the suite.
pub const DEFAULT_SEEDS: [u32; 3] = [12345, 67890, 11111];

fn default_seeds() -> Vec<u32> {
    DEFAULT_SEEDS.to_vec()
}

const fn default_include_control() -> bool {
    true
}

/// The temperature the control arm forces.
///
/// Chosen to be unambiguously bad for structured output rather than
/// marginally worse: the control's job is to produce a difference so large
/// that failing to detect it means the apparatus is not measuring sampling at
/// all. A subtle degradation would leave "no difference" ambiguous between a
/// broken harness and a robust model, which is the exact ambiguity this exists
/// to remove.
pub const CONTROL_TEMPERATURE: f32 = 2.0;

/// Which arm a task ran under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalArm {
    /// Pipeline bypassed — bare llama-server behaviour.
    Raw,
    /// The full gglib request/response pipeline.
    Gglib,
    /// **Positive control.** The gglib pipeline with the temperature forced to
    /// [`CONTROL_TEMPERATURE`], which should sample visibly worse.
    ///
    /// It exists to answer a question the other two arms cannot: *can this
    /// apparatus detect a sampling change at all?* A raw-versus-gglib run
    /// showing no difference has two explanations — the pipeline does not help,
    /// or the harness cannot see — and nothing in that run distinguishes them.
    ///
    /// This arm is a deliberate, large, known-bad change. If it does **not**
    /// score below the gglib arm, the apparatus failed to move under a
    /// difference that should be impossible to miss, and no other number in
    /// the report can be believed. That is the same discipline
    /// [ADR 0004](https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md)
    /// applies to its instruments: a comparison in which nothing could have
    /// varied, reporting that nothing varied, is not evidence.
    Control,
}

impl std::fmt::Display for EvalArm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Raw => write!(f, "raw"),
            Self::Gglib => write!(f, "gglib"),
            Self::Control => write!(f, "control"),
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
    /// How many seeds every task was repeated under.
    ///
    /// The sample size behind every mean above, and the thing that makes them
    /// comparable across runs. A composite from one seed and a composite from
    /// five are not the same measurement, and a report that renders them
    /// identically invites exactly the mistake this eval exists to prevent.
    ///
    /// `1` on a legacy row, which is what it was.
    #[serde(default = "one")]
    pub seeds: usize,
    /// Total task runs behind these scores — `tasks × seeds`.
    #[serde(default)]
    pub runs: usize,
}

const fn one() -> usize {
    1
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
///
/// Both sides carry **one entry per seed**, in seed order, rather than a single
/// result. Collapsing them to a representative run would hide the thing a
/// multi-seed eval is for: a task that passes 3/3 under one arm and 1/3 under
/// the other is a different finding from one that passes 3/3 versus 0/3, and
/// both render as "passed / failed" once the per-seed detail is gone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgenticTaskComparison {
    /// Task identifier from the suite.
    pub task_id: String,
    /// The task's BFCL-style category.
    pub category: TaskCategory,
    /// Per-seed results under the raw arm, in seed order.
    pub raw: Vec<TuneTaskResult>,
    /// Per-seed results under the gglib arm, in seed order.
    pub gglib: Vec<TuneTaskResult>,
}

impl AgenticTaskComparison {
    /// How many of this task's seeds passed under each arm.
    ///
    /// The per-task view of stability: `(2, 3)` means two of three seeds
    /// passed, which is a materially different claim from a bare `passed:
    /// true` taken from whichever seed happened to run first.
    #[must_use]
    pub fn pass_counts(&self) -> (usize, usize) {
        (
            self.raw.iter().filter(|r| r.passed).count(),
            self.gglib.iter().filter(|r| r.passed).count(),
        )
    }

    /// Whether either arm disagreed with itself across seeds.
    ///
    /// A task that flips between passing and failing on identical
    /// configuration is where suite-level variance comes from, and naming it
    /// per task is what turns "the numbers moved" into something actionable.
    #[must_use]
    pub fn is_unstable(&self) -> bool {
        let mixed = |runs: &[TuneTaskResult]| {
            runs.iter().any(|r| r.passed) && runs.iter().any(|r| !r.passed)
        };
        mixed(&self.raw) || mixed(&self.gglib)
    }
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
    /// The seeds every task ran under, in order. Empty on a legacy row and on
    /// an explicitly unseeded run.
    #[serde(default)]
    pub seeds: Vec<u32>,
    /// Scores under the positive control arm, when it ran.
    ///
    /// Read [`Self::control_moved`] rather than these numbers directly: what
    /// matters is not the control's score but whether it *differs*.
    #[serde(default)]
    pub control: Option<ArmScores>,
}

/// The smallest composite gap the control arm must open for the apparatus to
/// have demonstrably moved.
///
/// Not a quality bar — a *detection* bar. [`CONTROL_TEMPERATURE`] is chosen to
/// be unambiguously bad, so a gap smaller than this means the measurement did
/// not respond to a change that should have been impossible to miss.
pub const CONTROL_MIN_COMPOSITE_GAP: f64 = 0.05;

impl AgenticEvalReport {
    /// Whether the positive control demonstrated that this run could detect a
    /// sampling change.
    ///
    /// - `Some(true)` — the control scored at least
    ///   [`CONTROL_MIN_COMPOSITE_GAP`] below the gglib arm. The apparatus
    ///   moved under a known-bad change, so a null result elsewhere in this
    ///   report is evidence rather than silence.
    /// - `Some(false)` — it did not. **Every other number here is
    ///   uninterpretable**: the run cannot distinguish "no effect" from "no
    ///   sensitivity".
    /// - `None` — the control was not run, so nothing is claimed either way.
    ///   Distinct from `Some(false)` for the same reason `Blind` is distinct
    ///   from zero divergences.
    #[must_use]
    pub fn control_moved(&self) -> Option<bool> {
        self.control
            .as_ref()
            .map(|c| self.gglib.composite - c.composite >= CONTROL_MIN_COMPOSITE_GAP)
    }

    /// Tasks whose outcome was not stable across seeds under either arm.
    ///
    /// The direct read of run-to-run variance, and the first thing to look at
    /// when two arms differ by less than they ought to.
    #[must_use]
    pub fn unstable_tasks(&self) -> Vec<&AgenticTaskComparison> {
        self.tasks.iter().filter(|t| t.is_unstable()).collect()
    }

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
            seeds: 3,
            runs: 12,
        }
    }

    fn task_result(id: &str, passed: bool) -> TuneTaskResult {
        TuneTaskResult {
            task_id: id.to_owned(),
            category: TaskCategory::SingleCall,
            passed,
            tool_match_score: if passed { 1.0 } else { 0.0 },
            loop_detected: false,
            stagnation_detected: false,
            iterations: 1,
            latency_ms: 10,
            completion_tokens: Some(100),
            time_to_first_tool_call_ms: Some(5),
            detail: None,
        }
    }

    fn comparison(raw: &[bool], gglib: &[bool]) -> AgenticTaskComparison {
        AgenticTaskComparison {
            task_id: "t".to_owned(),
            category: TaskCategory::SingleCall,
            raw: raw.iter().map(|p| task_result("t", *p)).collect(),
            gglib: gglib.iter().map(|p| task_result("t", *p)).collect(),
        }
    }

    fn report_with(control: Option<ArmScores>, gglib_composite: f64) -> AgenticEvalReport {
        AgenticEvalReport {
            model_name: "m".to_owned(),
            quantization: None,
            param_count_b: 1.0,
            ctx_size: 4096,
            raw: scores(0.5, None, 0.5),
            gglib: scores(0.9, None, gglib_composite),
            delta: AgenticEvalReport::delta_of(
                &scores(0.5, None, 0.5),
                &scores(0.9, None, gglib_composite),
            ),
            tasks: vec![],
            seeds: DEFAULT_SEEDS.to_vec(),
            control,
        }
    }

    // =========================================================================
    // Seeds
    // =========================================================================

    /// The default has to be more than one, or the eval is back to reporting a
    /// single draw from the model's output distribution as a measurement.
    #[test]
    fn the_default_seed_set_is_larger_than_one() {
        assert!(DEFAULT_SEEDS.len() > 1);
        assert_eq!(default_seeds(), DEFAULT_SEEDS.to_vec());
    }

    /// A config written before seeds existed must still deserialize, and must
    /// pick up the multi-seed default rather than silently staying single.
    #[test]
    fn a_legacy_config_gains_the_default_seeds_and_control() {
        let json = r#"{"model_id": 1, "task_suite": {"source": "default"}}"#;
        let config: AgenticEvalConfig = serde_json::from_str(json).expect("deserializes");

        assert_eq!(config.seeds, DEFAULT_SEEDS.to_vec());
        assert!(config.include_control);
    }

    /// An explicitly empty seed list is a real choice — one unseeded run — and
    /// must survive as empty rather than being backfilled with the default.
    #[test]
    fn an_explicitly_empty_seed_list_stays_empty() {
        let json = r#"{"model_id": 1, "task_suite": {"source": "default"}, "seeds": []}"#;
        let config: AgenticEvalConfig = serde_json::from_str(json).expect("deserializes");

        assert!(config.seeds.is_empty());
    }

    /// A legacy stored report has no sample size recorded, and `1` is what it
    /// actually was — not zero, which would render as "no runs".
    #[test]
    fn a_legacy_report_reads_as_a_single_seed() {
        let json = r#"{
            "tool_accuracy": 0.5, "task_completion": 0.5, "composite": 0.5
        }"#;
        let scores: ArmScores = serde_json::from_str(json).expect("deserializes");

        assert_eq!(scores.seeds, 1);
    }

    // =========================================================================
    // Per-task stability
    // =========================================================================

    /// The finding a multi-seed eval exists to surface: a task that passes
    /// under one arm on every seed and under the other on only some.
    #[test]
    fn pass_counts_report_each_arms_seeds_separately() {
        let cmp = comparison(&[true, false, false], &[true, true, true]);
        assert_eq!(cmp.pass_counts(), (1, 3));
    }

    /// A task that disagrees with itself across seeds is where suite variance
    /// comes from, and it must be findable whichever arm was unstable.
    #[test]
    fn a_task_that_flips_between_seeds_is_unstable() {
        assert!(comparison(&[true, false], &[true, true]).is_unstable());
        assert!(comparison(&[true, true], &[false, true]).is_unstable());
    }

    /// Consistent outcomes are stable even when the two arms disagree with
    /// each other — that is a *result*, not instability.
    #[test]
    fn arms_disagreeing_consistently_is_not_instability() {
        assert!(!comparison(&[false, false, false], &[true, true, true]).is_unstable());
        assert!(!comparison(&[true, true], &[true, true]).is_unstable());
    }

    // =========================================================================
    // The positive control
    // =========================================================================

    /// The control's job: a known-bad sampling change must register, or the
    /// run proved nothing about its own sensitivity.
    #[test]
    fn a_control_that_scored_well_below_gglib_demonstrates_sensitivity() {
        let report = report_with(Some(scores(0.2, None, 0.30)), 0.90);
        assert_eq!(report.control_moved(), Some(true));
    }

    /// **The failure this exists to catch.** Forcing temperature 2.0 barely
    /// changed the score, so the apparatus cannot detect a sampling change —
    /// and therefore cannot support any other delta in the report.
    #[test]
    fn a_control_that_barely_moved_reports_failure() {
        let report = report_with(Some(scores(0.88, None, 0.89)), 0.90);
        assert_eq!(report.control_moved(), Some(false));
    }

    /// A control scoring *above* the gglib arm is a failure too: the change
    /// was known-bad, so this is not a sensitivity demonstration either.
    #[test]
    fn a_control_that_scored_higher_is_not_a_demonstration() {
        let report = report_with(Some(scores(0.95, None, 0.99)), 0.90);
        assert_eq!(report.control_moved(), Some(false));
    }

    /// Not run is not the same as ran-and-failed — the same distinction the
    /// sampling readback draws between blind and zero divergences.
    #[test]
    fn no_control_arm_claims_nothing_either_way() {
        assert_eq!(report_with(None, 0.90).control_moved(), None);
    }

    /// The threshold has to be a real gap, not any difference at all, or noise
    /// would satisfy the control.
    #[test]
    fn the_control_threshold_is_larger_than_rounding() {
        const { assert!(CONTROL_MIN_COMPOSITE_GAP > 0.0) };
        let exactly_at = report_with(
            Some(scores(0.5, None, 0.90 - CONTROL_MIN_COMPOSITE_GAP)),
            0.90,
        );
        assert_eq!(
            exactly_at.control_moved(),
            Some(true),
            "the bound is inclusive"
        );
    }

    /// The control must be unambiguously bad rather than marginally worse, or
    /// "no difference" stays ambiguous between a broken harness and a robust
    /// model.
    #[test]
    fn the_control_temperature_is_far_outside_any_sane_recipe() {
        const { assert!(CONTROL_TEMPERATURE >= 2.0) };
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
