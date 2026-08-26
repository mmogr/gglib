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
//!
//! Two further arms exist to keep those deltas honest, and neither is a
//! measurement of the pipeline:
//!
//! - **`raw_replicate`** ([`EvalArm::RawReplicate`]) runs the raw arm a second
//!   time on a *disjoint* seed set. Nothing differs between it and the raw arm
//!   except which seeds were drawn, so whatever gap it opens is the eval's own
//!   drift — the floor a raw-versus-gglib delta has to clear before it means
//!   anything. An A/A test.
//! - **`control`** ([`EvalArm::Control`]) runs the gglib pipeline with sampling
//!   deliberately broken, and must score far below it. It answers the opposite
//!   question: not *is this difference real* but *could this apparatus have
//!   seen a difference at all*.
//!
//! They answer different failures and neither substitutes for the other. A
//! control that moves 0.5 says the eval can detect a large change; it says
//! nothing about whether it can resolve a 0.08 one, which is what the A/A arm
//! is for.

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
    ///
    /// `None` means "the server decides"; see [`TuneConfig::weights`], which
    /// also explains why `skip_serializing_if` is required rather than
    /// cosmetic.
    ///
    /// [`TuneConfig::weights`]: super::tune::config::TuneConfig::weights
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights: Option<ScoreWeights>,
    /// Context size override (tokens). `None` resolves through the harness's
    /// own chain (model server defaults → global setting → hardcoded default),
    /// which deliberately stops short of the fitted rung a real launch reaches
    /// — so a benchmark taken with nothing configured is taken at the floor.
    /// Recorded in ADR 0009's amendment; not the serving path's chain.
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
    /// Whether to run the A/A arm. See [`EvalArm::RawReplicate`].
    ///
    /// On by default, and cheap: the raw arm is the fastest of the three, so
    /// repeating it costs a fraction of what the control does and is the only
    /// thing in the report that speaks to the *size* of an effect rather than
    /// its direction.
    #[serde(default = "default_replicate_raw")]
    pub replicate_raw: bool,
    /// How many A/A pairs to run. See [`EvalArm::RawReplicate`].
    ///
    /// `1` is the historical single-pair behaviour and the default. A single
    /// pair estimates the eval's drift from one degree of freedom — enough to
    /// stop a delta inside its own noise being called a finding, and not
    /// enough to say how noisy the eval actually is. Every additional pair
    /// re-runs the raw arm on another derived, disjoint seed set, and the
    /// drift estimate becomes the mean pairwise gap over all replicate runs
    /// plus the primary — which is the "more pairs" the
    /// [`EFFECT_NOISE_RATIO`] doc has always named as the honest
    /// strengthening.
    #[serde(default = "default_replicate_pairs")]
    pub replicate_pairs: usize,
    /// How many of [`Self::seeds`] the positive control repeats, from the
    /// front. Clamped into `1..=seeds.len()`.
    ///
    /// # Why this is not the full seed set
    ///
    /// Because the control is the most expensive arm in the eval by an order
    /// of magnitude, and it does not need the precision. Measured on
    /// Qwen3.5-4B: broken sampling makes the model ramble, so the control took
    /// **161 of one run's 174 wall-clock minutes** and generated 5× the tokens
    /// of the two real arms combined.
    ///
    /// It can afford to be imprecise because of what it is asked. The two real
    /// arms are being compared to each other and need every seed they can get;
    /// the control only has to clear [`CONTROL_MIN_COMPOSITE_GAP`], and the gap
    /// it actually opens is an order of magnitude above that threshold. Paying
    /// five seeds to resolve a 0.5 gap more precisely buys nothing the report
    /// reads.
    #[serde(default = "default_control_seeds")]
    pub control_seeds: usize,
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

const fn default_replicate_raw() -> bool {
    true
}

const fn default_control_seeds() -> usize {
    1
}

const fn default_replicate_pairs() -> usize {
    1
}

/// Offset added to each primary seed to derive the A/A arm's seeds.
///
/// The 32-bit golden-ratio constant, chosen for nothing but being a fixed,
/// well-spread, unremarkable number. Derived rather than drawn because the A/A
/// arm has to be as reproducible as the arms it is calibrating: a noise floor
/// that changes every run cannot be compared against anything.
pub const REPLICATE_SEED_OFFSET: u32 = 0x9E37_79B9;

/// The seed set the A/A arm runs, derived from the primary one.
///
/// # Why the seeds must differ
///
/// This is the whole design of the arm. Re-running the *same* seeds would
/// measure how reproducible a fixed seed is — which, given a deterministic
/// decode, is approximately "perfectly", and would report a noise floor near
/// zero. That number is true and useless: the primary comparison's precision
/// is not limited by whether seed `12345` replays, it is limited by *which five
/// seeds happened to be drawn*. So the replicate draws five different ones and
/// measures exactly that.
///
/// A pathological seed list can still overlap — `[1, 1 + OFFSET]` maps onto
/// itself by one element — so the seeds the replicate actually used are
/// recorded in [`AgenticEvalReport::replicate_seeds`] rather than left implicit.
#[must_use]
pub fn replicate_seeds(seeds: &[u32]) -> Vec<u32> {
    replicate_seed_set(seeds, 1)
}

/// The seed set for A/A pair `pair` (1-based): the primary seeds offset by
/// `pair` strides of [`REPLICATE_SEED_OFFSET`].
///
/// Pair 1 is exactly [`replicate_seeds`], so a multi-pair run's first pair
/// reproduces the single-pair run's numbers. Strides of a fixed constant
/// rather than fresh draws for the same reason the offset itself is fixed: a
/// noise floor that changes every run cannot be compared against anything.
#[must_use]
pub fn replicate_seed_set(seeds: &[u32], pair: u32) -> Vec<u32> {
    seeds
        .iter()
        .map(|seed| seed.wrapping_add(REPLICATE_SEED_OFFSET.wrapping_mul(pair)))
        .collect()
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

/// `top_k` the control arm forces. `0` disables the cut entirely.
pub const CONTROL_TOP_K: i32 = 0;

/// `top_p` the control arm forces. `1.0` keeps the whole nucleus.
pub const CONTROL_TOP_P: f32 = 1.0;

/// `min_p` the control arm forces. `0.0` disables the tail cut.
pub const CONTROL_MIN_P: f32 = 0.0;

/// The sampling the control arm applies, on top of a request's seed.
///
/// # Why the temperature alone was not enough
///
/// The first version of this control set only [`CONTROL_TEMPERATURE`], and it
/// **failed to degrade anything** — measured on Qwen3.5-4B, it scored *above*
/// both real arms. The reason is the sampler chain's order, which [ADR 0003]
/// finding 5 measured: llama.cpp applies the truncation samplers *before*
/// temperature. With a `reasoning` recipe's `top_k: 20` and `top_p: 0.95`
/// already in force, temperature 2.0 was only flattening a distribution over
/// twenty surviving tokens — a much tamer change than the number suggests.
///
/// So the control disables every truncation sampler as well. A temperature
/// that cannot be absorbed by a `top_k` running ahead of it is the only kind
/// that demonstrates anything.
///
/// # It differs from the gglib arm in more than one value, and that is fine
///
/// An earlier comment here claimed the control differed in exactly the
/// temperature, so a gap could only be that. That was already untrue: naming a
/// temperature claims the coupled trio, so the control's `presence_penalty`
/// and `repeat_penalty` fall to the class floor rather than matching the
/// model's recipe. Isolating one variable is a job for an ablation; this
/// arm's job is to be *large and known-bad*, and breadth serves that.
///
/// [ADR 0003]: https://github.com/mmogr/gglib/blob/main/docs/adr/0003-defer-sampler-defaults-to-llama-cpp.md
#[must_use]
pub const fn control_sampling() -> (f32, i32, f32, f32) {
    (
        CONTROL_TEMPERATURE,
        CONTROL_TOP_K,
        CONTROL_TOP_P,
        CONTROL_MIN_P,
    )
}

/// Which arm a task ran under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalArm {
    /// Pipeline bypassed — bare llama-server behaviour.
    Raw,
    /// The full gglib request/response pipeline.
    Gglib,
    /// **A/A control.** The raw arm again, on a disjoint seed set.
    ///
    /// Nothing about the request differs from [`Self::Raw`] — same bypass, same
    /// tasks, same machine, same loaded model — so any gap between the two is
    /// the eval measuring itself. That gap is the floor a raw-versus-gglib
    /// delta has to clear, and without it a small delta has two readings that
    /// the report cannot separate: the pipeline helped a little, or five seeds
    /// is not enough seeds.
    ///
    /// It answers a strictly different question from [`Self::Control`]. The
    /// control establishes that a *large* change registers; this establishes
    /// how large a change has to be before it registers as anything but drift.
    /// A run carrying only the control can say "the apparatus works" about an
    /// effect it has no ability to resolve.
    RawReplicate,
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
            Self::RawReplicate => write!(f, "raw (A/A)"),
            Self::Control => write!(f, "control"),
        }
    }
}

/// One arm's aggregate scores across the task suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub total_completion_tokens: Option<u64>,
    /// Total wall-clock milliseconds across every task in the suite,
    /// unfiltered — the honest cost of running it.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
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
    /// How many of those runs never reached the model, and therefore
    /// contributed a zero that measures nothing.
    ///
    /// See [`TuneTaskResult::unmeasured`]. An arm where this equals
    /// [`Self::runs`] is not a low score — it is an empty column, and the eval
    /// refuses to report one rather than rendering it as an arm that did
    /// badly. Anything between `1` and `runs` contaminates every mean above by
    /// an amount this number is the only record of.
    #[serde(default)]
    pub unmeasured_runs: usize,
}

const fn one() -> usize {
    1
}

/// Per-axis difference, `gglib − raw`. Positive means gglib scored higher.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct AgenticEvalReport {
    /// Model name as stored in the catalog.
    pub model_name: String,
    /// Quantization label (e.g. `Q4_K_M`), when known.
    pub quantization: Option<String>,
    /// Parameter count in billions.
    pub param_count_b: f64,
    /// Context size both arms ran at, in tokens.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
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
    /// Read [`Self::control_verdict`] rather than these numbers directly: what
    /// matters is not the control's score but whether it *differs*. Its
    /// [`ArmScores::seeds`] is usually smaller than the real arms' — see
    /// [`AgenticEvalConfig::control_seeds`] — so its composite is a coarser
    /// number than the ones it sits beside.
    #[serde(default)]
    pub control: Option<ArmScores>,
    /// Scores under the A/A arm — the raw pipeline again, different seeds.
    ///
    /// Read [`Self::effect_verdict`] rather than this directly: the number that
    /// matters is its *distance* from [`Self::raw`], not its own value.
    #[serde(default)]
    pub raw_replicate: Option<ArmScores>,
    /// The seeds the A/A arm ran under. Empty when it did not run, and when it
    /// ran unseeded.
    ///
    /// Recorded rather than derived at read time so an overlap with
    /// [`Self::seeds`] is visible in the report instead of having to be
    /// recomputed from [`replicate_seeds`].
    #[serde(default)]
    pub replicate_seeds: Vec<u32>,
    /// Every A/A pair's scores, in pair order, when more than one ran.
    ///
    /// [`Self::raw_replicate`] stays populated with the first pair so a
    /// single-pair report — and every report written before this field —
    /// reads exactly as it always did. A legacy row deserializes this empty,
    /// and [`Self::noise_floor`] falls back to the single pair.
    #[serde(default)]
    pub raw_replicates: Vec<ArmScores>,
    /// The seed set behind each entry of [`Self::raw_replicates`].
    #[serde(default)]
    pub replicate_seed_sets: Vec<Vec<u32>>,
    /// The paired per-`(task, seed)` comparison, computed at assembly.
    ///
    /// Stored rather than derived-only, unlike the verdicts: those re-derive
    /// from two floats in any language, while this one carries a rank test
    /// nobody should maintain twice. [`Self::paired_effect`] re-derives it
    /// from the drill-down for reports written before the field existed.
    #[serde(default)]
    pub paired: Option<PairedEffect>,
}

/// The smallest composite gap the control arm must open for the apparatus to
/// have demonstrably moved.
///
/// Not a quality bar — a *detection* bar. [`CONTROL_TEMPERATURE`] is chosen to
/// be unambiguously bad, so a gap smaller than this means the measurement did
/// not respond to a change that should have been impossible to miss.
pub const CONTROL_MIN_COMPOSITE_GAP: f64 = 0.05;

/// What the positive control demonstrated about this run's sensitivity.
///
/// Three outcomes rather than a bool, because the two ways of failing mean
/// different things and want different fixes. Collapsing them is what made a
/// real 0.090 swing render as "changed by only -0.090" — wording that reads as
/// *barely moved* about a control that moved a great deal, in the wrong
/// direction. That is [ADR 0004] decision 3's rule applied to a verdict rather
/// than to a field: a state that licenses a different action must render
/// differently.
///
/// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum ControlVerdict {
    /// The control scored at least [`CONTROL_MIN_COMPOSITE_GAP`] below the
    /// gglib arm. The apparatus moved under a known-bad change, so a null
    /// result elsewhere in the report is evidence rather than silence.
    Moved {
        /// `gglib − control`, positive.
        gap: f64,
    },
    /// The control scored below the gglib arm but by less than the threshold.
    /// The apparatus may simply be insensitive at this suite size.
    TooSmall {
        /// `gglib − control`, positive but under the threshold.
        gap: f64,
    },
    /// **The control scored *higher* than the gglib arm.**
    ///
    /// Not a weak signal — a contradicted premise. The change was chosen to be
    /// bad, so a control that wins says the degradation is not degrading, and
    /// the control itself needs fixing before any delta in the report means
    /// anything. Measured once already: temperature 2.0 without disabling
    /// `top_k` is absorbed by the truncation samplers that run ahead of it.
    WrongDirection {
        /// How far *above* the gglib arm the control scored, positive.
        gap: f64,
    },
}

impl ControlVerdict {
    /// Whether this run demonstrated it could detect a sampling change.
    #[must_use]
    pub const fn demonstrated_sensitivity(&self) -> bool {
        matches!(self, Self::Moved { .. })
    }
}

/// How many times the raw-versus-gglib effect must exceed the A/A drift before
/// the report will call it more than noise.
///
/// # This is a rule of thumb, not a test
///
/// A single A/A pair estimates the drift from one degree of freedom. Two draws
/// of a noisy quantity can land close together by luck, and a factor derived
/// from them carries no confidence level, no *p*, and no power. `2.0` is chosen
/// because it is the smallest factor at which the two numbers are plainly not
/// the same size — enough to stop a delta that is *within* its own noise from
/// being reported as a finding, and not enough to license the word
/// "significant" about one that clears it.
///
/// The honest way to strengthen this is more A/A pairs, not a bigger factor.
pub const EFFECT_NOISE_RATIO: f64 = 2.0;

/// What the A/A arm says about the size of the measured effect.
///
/// Deliberately not a p-value or a confidence interval: with one replicate
/// there is nothing to compute either from, and rendering a statistic that the
/// design cannot support would be worse than rendering none. The two arms of
/// this enum are the honest resolution of a one-pair comparison — the effect is
/// clearly bigger than the drift, or it is not clearly bigger than the drift.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum EffectVerdict {
    /// `|gglib − raw|` is at least [`EFFECT_NOISE_RATIO`] times the A/A gap.
    ExceedsNoise {
        /// `gglib − raw`, signed: a negative effect that clears the noise floor
        /// is still a finding, just not the hoped-for one.
        effect: f64,
        /// The mean pairwise drift among identical raw runs.
        noise: f64,
        /// Pairwise gaps behind `noise` — the estimate's degrees of freedom.
        /// A verdict over one pair and one over six are not the same strength
        /// of claim, and rendering them identically invites exactly the
        /// misreading the A/A arm exists to prevent.
        #[serde(default = "one")]
        pairs: usize,
    },
    /// The effect is not clearly larger than the drift between two runs of the
    /// same arm. It is not thereby *absent* — it is unresolved at this seed
    /// count, and the fix is more seeds rather than a different conclusion.
    WithinNoise {
        /// `gglib − raw`, signed.
        effect: f64,
        /// The mean pairwise drift among identical raw runs.
        noise: f64,
        /// Pairwise gaps behind `noise`.
        #[serde(default = "one")]
        pairs: usize,
    },
}

impl EffectVerdict {
    /// `|effect| ÷ noise`, or `None` when the two arms landed on exactly the
    /// same composite and the ratio would divide by zero.
    ///
    /// A zero denominator is not a licence to report an infinite ratio: two
    /// identical scores on a suite this small means the drift went unresolved,
    /// not that there is none.
    #[must_use]
    pub fn ratio(&self) -> Option<f64> {
        let (effect, noise) = match *self {
            Self::ExceedsNoise { effect, noise, .. } | Self::WithinNoise { effect, noise, .. } => {
                (effect, noise)
            }
        };
        (noise > 0.0).then(|| effect.abs() / noise)
    }

    /// The signed `gglib − raw` difference this verdict is about.
    #[must_use]
    pub const fn effect(&self) -> f64 {
        match *self {
            Self::ExceedsNoise { effect, .. } | Self::WithinNoise { effect, .. } => effect,
        }
    }

    /// How many pairwise drift gaps stand behind [`Self::noise`].
    #[must_use]
    pub const fn pairs(&self) -> usize {
        match *self {
            Self::ExceedsNoise { pairs, .. } | Self::WithinNoise { pairs, .. } => pairs,
        }
    }

    /// The A/A drift this verdict measured the effect against.
    #[must_use]
    pub const fn noise(&self) -> f64 {
        match *self {
            Self::ExceedsNoise { noise, .. } | Self::WithinNoise { noise, .. } => noise,
        }
    }

    /// Whether the effect cleared the drift by [`EFFECT_NOISE_RATIO`].
    #[must_use]
    pub const fn exceeds_noise(&self) -> bool {
        matches!(self, Self::ExceedsNoise { .. })
    }
}

impl ArmScores {
    /// Whether **no** run in this arm reached the model.
    ///
    /// The state that must never render as a score. An arm in it has a
    /// composite, a tool accuracy and a task completion, all arithmetically
    /// correct and all meaningless — computed over runs that produced no
    /// response to score.
    #[must_use]
    pub const fn is_empty_column(&self) -> bool {
        self.runs > 0 && self.unmeasured_runs >= self.runs
    }

    /// Whether *some* but not all of this arm's runs reached the model.
    ///
    /// Distinct from [`Self::is_empty_column`] because it wants a different
    /// action: the arm has real observations mixed with empty ones, so its
    /// means are contaminated by a knowable amount rather than vacant.
    #[must_use]
    pub const fn is_partly_unmeasured(&self) -> bool {
        self.unmeasured_runs > 0 && self.unmeasured_runs < self.runs
    }
}

impl AgenticEvalReport {
    /// What the positive control demonstrated, or `None` when it did not run.
    ///
    /// `None` is distinct from any failure for the same reason `Blind` is
    /// distinct from zero divergences: nothing was claimed either way.
    #[must_use]
    pub fn control_verdict(&self) -> Option<ControlVerdict> {
        let control = self.control.as_ref()?;
        let gap = self.gglib.composite - control.composite;
        Some(if gap >= CONTROL_MIN_COMPOSITE_GAP {
            ControlVerdict::Moved { gap }
        } else if gap >= 0.0 {
            ControlVerdict::TooSmall { gap }
        } else {
            ControlVerdict::WrongDirection { gap: -gap }
        })
    }

    /// The eval's own drift: the mean pairwise composite gap over every run
    /// of the identical raw configuration — the primary plus each A/A pair.
    ///
    /// With one A/A pair this is exactly the old single-gap number. With `K`
    /// pairs it averages the `C(K+1, 2)` pairwise gaps among `K + 1` runs of
    /// the same arm, which estimates the same quantity from more than one
    /// degree of freedom. A mean absolute gap, not a standard deviation:
    /// [`EFFECT_NOISE_RATIO`] was calibrated against a gap, and changing the
    /// estimator and the threshold at once would make old and new verdicts
    /// incomparable.
    ///
    /// `None` when no A/A arm ran, which is distinct from a measured zero for
    /// the same reason `Blind` is distinct from zero divergences.
    #[must_use]
    pub fn noise_floor(&self) -> Option<f64> {
        let gaps = self.drift_gaps();
        #[allow(clippy::cast_precision_loss)]
        match gaps.len() {
            0 => None,
            n => Some(gaps.iter().sum::<f64>() / n as f64),
        }
    }

    /// How many pairwise gaps stand behind [`Self::noise_floor`] — the
    /// degrees of freedom a reader should weigh the verdict by.
    #[must_use]
    pub fn noise_pairs(&self) -> usize {
        self.drift_gaps().len()
    }

    /// Pairwise absolute composite gaps among every run of the raw
    /// configuration. Empty when no A/A arm ran.
    fn drift_gaps(&self) -> Vec<f64> {
        let mut composites = vec![self.raw.composite];
        if self.raw_replicates.is_empty() {
            if let Some(replicate) = self.raw_replicate.as_ref() {
                composites.push(replicate.composite);
            }
        } else {
            composites.extend(self.raw_replicates.iter().map(|r| r.composite));
        }
        let mut gaps = Vec::new();
        for (i, a) in composites.iter().enumerate() {
            for b in &composites[i + 1..] {
                gaps.push((a - b).abs());
            }
        }
        gaps
    }

    /// Whether the measured effect is larger than the eval's own drift.
    ///
    /// `None` when no A/A arm ran — in which case the report contains no basis
    /// for the judgement at all, and the composite delta above it should be
    /// read as a direction rather than as a magnitude.
    #[must_use]
    pub fn effect_verdict(&self) -> Option<EffectVerdict> {
        let noise = self.noise_floor()?;
        let effect = self.delta.composite;
        // A zero effect never "exceeds" anything, however quiet the arm was:
        // with both terms at zero the inequality would hold vacuously and
        // report no difference as a finding.
        let pairs = self.noise_pairs();
        Some(
            if effect.abs() > 0.0 && effect.abs() >= EFFECT_NOISE_RATIO * noise {
                EffectVerdict::ExceedsNoise {
                    effect,
                    noise,
                    pairs,
                }
            } else {
                EffectVerdict::WithinNoise {
                    effect,
                    noise,
                    pairs,
                }
            },
        )
    }

    /// Tasks whose outcome was not stable across seeds under either arm.
    ///
    /// The direct read of run-to-run variance, and the first thing to look at
    /// when two arms differ by less than they ought to.
    #[must_use]
    pub fn unstable_tasks(&self) -> Vec<&AgenticTaskComparison> {
        self.tasks.iter().filter(|t| t.is_unstable()).collect()
    }

    /// The paired per-`(task, seed)` comparison, derived from the drill-down.
    ///
    /// Derived rather than stored, like the verdicts above it — which also
    /// means a legacy report's stored per-seed detail yields it retroactively.
    /// `None` when no pair has both sides measured.
    #[must_use]
    pub fn paired_effect(&self) -> Option<PairedEffect> {
        self.paired
            .or_else(|| PairedEffect::from_tasks(&self.tasks))
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

/// The paired view of the raw-versus-gglib comparison.
///
/// The two real arms run the **same seeds on the same tasks**, so every
/// `(task, seed)` cell is a matched pair — and pairing is what removes the
/// eval's identical-arm spread from the comparison. The ceiling experiment
/// (tune runs #12–#32, ADR 0004's postscript) resolved a +0.067 effect
/// through noise wider than that *only* because it paired per run; the same
/// data has been sitting in [`AgenticEvalReport::tasks`] all along, compared
/// only as arm means.
///
/// Pairs are on [`TuneTaskResult::tool_match_score`] — the one graded
/// per-run quality scalar. Pass/fail flips remain visible per task in
/// [`AgenticTaskComparison::pass_counts`]; folding them in here would double
/// count, since the match score is most of what decides `passed`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct PairedEffect {
    /// Matched `(task, seed)` pairs in which both arms produced a real
    /// observation.
    pub pairs: usize,
    /// Pairs both arms ran but at least one side never reached the model —
    /// dropped from every number here, and reported so the drop is visible.
    pub unmeasured_pairs: usize,
    /// Pairs the gglib arm scored strictly higher.
    pub wins: usize,
    /// Pairs the raw arm scored strictly higher.
    pub losses: usize,
    /// Pairs with identical scores. On a suite where most tasks pass cleanly
    /// under both arms this is the largest bucket, and that is information:
    /// the arms mostly agree.
    pub ties: usize,
    /// Mean of `gglib − raw` over the measured pairs.
    pub mean_delta: f64,
    /// One-sided Wilcoxon signed-rank *p* for "gglib scores higher", by
    /// normal approximation with tie correction.
    ///
    /// `None` below [`WILCOXON_MIN_PAIRS`] non-tied pairs — the approximation
    /// is not trustworthy there, and rendering a statistic the design cannot
    /// support is worse than rendering none (the [`EffectVerdict`] rule). At
    /// small counts, read [`Self::wins`] against [`Self::losses`] instead.
    pub p_value: Option<f64>,
}

/// The fewest non-tied pairs the normal-approximation Wilcoxon accepts.
///
/// Below this the approximation's error is material and an exact table would
/// be needed; above it the correction terms keep it honest.
pub const WILCOXON_MIN_PAIRS: usize = 8;

impl PairedEffect {
    /// Compute the paired comparison from the per-task drill-down.
    ///
    /// `None` when no `(task, seed)` pair has both sides measured — a paired
    /// analysis of nothing is not a zero effect.
    #[must_use]
    pub fn from_tasks(tasks: &[AgenticTaskComparison]) -> Option<Self> {
        let mut deltas = Vec::new();
        let mut unmeasured_pairs = 0_usize;
        for task in tasks {
            for (raw, gglib) in task.raw.iter().zip(task.gglib.iter()) {
                if raw.is_measured() && gglib.is_measured() {
                    deltas.push(gglib.tool_match_score - raw.tool_match_score);
                } else {
                    unmeasured_pairs += 1;
                }
            }
        }
        Self::from_deltas(&deltas, unmeasured_pairs)
    }

    /// The paired comparison between two runs of the same task list, paired
    /// by `task_id` — the first argument's score minus the second's, so
    /// `wins` counts pairs the *first* run took.
    ///
    /// Built for the tune apply gate (winner versus incumbent), where the
    /// two sides are candidates rather than eval arms. A task present in one
    /// run and absent from the other is skipped, not counted: an unpaired
    /// task has nothing to compare.
    #[must_use]
    pub fn from_paired_runs(a: &[TuneTaskResult], b: &[TuneTaskResult]) -> Option<Self> {
        let b_by_id: std::collections::HashMap<&str, &TuneTaskResult> =
            b.iter().map(|r| (r.task_id.as_str(), r)).collect();
        let mut deltas = Vec::new();
        let mut unmeasured_pairs = 0_usize;
        for left in a {
            let Some(right) = b_by_id.get(left.task_id.as_str()) else {
                continue;
            };
            if left.is_measured() && right.is_measured() {
                deltas.push(left.tool_match_score - right.tool_match_score);
            } else {
                unmeasured_pairs += 1;
            }
        }
        Self::from_deltas(&deltas, unmeasured_pairs)
    }

    /// Aggregate a delta list into the paired record. `None` on no deltas —
    /// a paired analysis of nothing is not a zero effect.
    fn from_deltas(deltas: &[f64], unmeasured_pairs: usize) -> Option<Self> {
        if deltas.is_empty() {
            return None;
        }

        let wins = deltas.iter().filter(|d| **d > 0.0).count();
        let losses = deltas.iter().filter(|d| **d < 0.0).count();
        let ties = deltas.len() - wins - losses;
        #[allow(clippy::cast_precision_loss)]
        let mean_delta = deltas.iter().sum::<f64>() / deltas.len() as f64;

        Some(Self {
            pairs: deltas.len(),
            unmeasured_pairs,
            wins,
            losses,
            ties,
            mean_delta,
            p_value: wilcoxon_one_sided(deltas),
        })
    }
}

/// One-sided Wilcoxon signed-rank *p* for "the deltas are positive".
///
/// Textbook construction: zeros dropped, absolute deltas ranked with average
/// ranks over ties, `W⁻` (the rank sum of the negative deltas) compared
/// against its null distribution by normal approximation with the tie
/// correction and a continuity correction. Small `W⁻` — losses carrying
/// little rank weight — yields small *p*.
///
/// `None` when fewer than [`WILCOXON_MIN_PAIRS`] non-zero deltas remain.
fn wilcoxon_one_sided(deltas: &[f64]) -> Option<f64> {
    let mut nonzero: Vec<f64> = deltas.iter().copied().filter(|d| *d != 0.0).collect();
    let n = nonzero.len();
    if n < WILCOXON_MIN_PAIRS {
        return None;
    }
    nonzero.sort_by(|a, b| a.abs().partial_cmp(&b.abs()).expect("scores are finite"));

    // Average ranks over runs of tied |delta|, accumulating the tie
    // correction term as each run closes.
    let mut w_minus = 0.0_f64;
    let mut tie_correction = 0.0_f64;
    let mut index = 0;
    while index < n {
        let mut end = index + 1;
        // Bitwise equality is the right tie test here: ranks tie when the
        // stored |delta| values are literally the same number, and a margin
        // would invent ties between distinct scores.
        while end < n && (nonzero[end].abs() - nonzero[index].abs()).abs() == 0.0 {
            end += 1;
        }
        #[allow(clippy::cast_precision_loss)]
        let average_rank = ((index + 1 + end) as f64) / 2.0;
        let run = end - index;
        if run > 1 {
            #[allow(clippy::cast_precision_loss)]
            let t = run as f64;
            tie_correction += (t * t).mul_add(t, -t);
        }
        for value in &nonzero[index..end] {
            if *value < 0.0 {
                w_minus += average_rank;
            }
        }
        index = end;
    }

    #[allow(clippy::cast_precision_loss)]
    let nf = n as f64;
    let mean = nf * (nf + 1.0) / 4.0;
    let variance = nf * (nf + 1.0) * 2.0f64.mul_add(nf, 1.0) / 24.0 - tie_correction / 48.0;
    if variance <= 0.0 {
        // Every |delta| identical and tied: the statistic is degenerate, and
        // the sign test the caller can read from wins/losses is the honest
        // fallback.
        return None;
    }
    // Continuity correction toward the mean; "gglib higher" means W⁻ is
    // small, so the one-sided p is the lower tail.
    let z = (w_minus - mean + 0.5) / variance.sqrt();
    Some(normal_cdf(z))
}

/// Standard normal CDF via Abramowitz–Stegun 7.1.26 on `erf`, accurate to
/// ~1.5e-7 — orders of magnitude finer than any decision read from a *p*.
fn normal_cdf(z: f64) -> f64 {
    let x = z / std::f64::consts::SQRT_2;
    let t = 1.0 / 0.327_591_1f64.mul_add(x.abs(), 1.0);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    let signed = if x < 0.0 { -erf } else { erf };
    0.5 * (1.0 + signed)
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

    /// The sibling of the `TuneConfig` assertion in `tune::config` — see there
    /// for why an absent `weights` key and a `null` one are not interchangeable
    /// on the wire. `gglib benchmark agentic` has no weight flags at all, so
    /// every config the CLI builds carries `weights: None`; if `null` were the
    /// spelling serde produced, every agentic run would be the broken case.
    /// (The server re-serializes this same type into `benchmark_runs`, where
    /// `weights` is `Some` for any client that sent one.)
    #[test]
    fn a_config_with_no_weights_omits_the_key_rather_than_nulling_it() {
        let config: AgenticEvalConfig =
            serde_json::from_str(r#"{"model_id": 1, "task_suite": {"source": "default"}}"#)
                .expect("minimal body deserializes");
        assert!(config.weights.is_none());

        let body = serde_json::to_value(&config).expect("serializes");
        assert!(
            !body.as_object().expect("object").contains_key("weights"),
            "serialized body must not carry a `weights` key: {body}"
        );
    }

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
            unmeasured_runs: 0,
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
            unmeasured: None,
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
            raw_replicate: None,
            replicate_seeds: vec![],
            raw_replicates: vec![],
            replicate_seed_sets: vec![],
            paired: None,
        }
    }

    /// A report whose raw and A/A arms differ by `noise` and whose gglib arm
    /// sits `effect` above raw, with everything else held fixed.
    fn report_with_replicate(effect: f64, noise: f64) -> AgenticEvalReport {
        let raw = scores(0.5, None, 0.500);
        let gglib = scores(0.9, None, 0.500 + effect);
        let replicate = scores(0.5, None, 0.500 + noise);
        AgenticEvalReport {
            delta: AgenticEvalReport::delta_of(&raw, &gglib),
            raw,
            gglib,
            raw_replicate: Some(replicate),
            replicate_seeds: replicate_seeds(&DEFAULT_SEEDS),
            ..report_with(None, 0.9)
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
        assert!(config.replicate_raw, "the A/A arm is on by default");
        assert_eq!(
            config.control_seeds, 1,
            "the control does not pay for precision it is never read for"
        );
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
        let verdict = report_with(Some(scores(0.2, None, 0.30)), 0.90).control_verdict();
        assert!(matches!(verdict, Some(ControlVerdict::Moved { .. })));
    }

    /// **The failure this exists to catch.** Forcing temperature 2.0 barely
    /// changed the score, so the apparatus cannot detect a sampling change —
    /// and therefore cannot support any other delta in the report.
    #[test]
    fn a_control_that_barely_moved_reports_failure() {
        let verdict = report_with(Some(scores(0.88, None, 0.89)), 0.90).control_verdict();
        assert!(matches!(verdict, Some(ControlVerdict::TooSmall { .. })));
    }

    /// **The failure that was measured, and that the old bool hid.** A control
    /// scoring *above* the gglib arm contradicts its premise rather than
    /// failing a threshold, and it must not be reported as "barely moved".
    #[test]
    fn a_control_that_scored_higher_is_its_own_verdict() {
        let report = report_with(Some(scores(0.95, None, 0.99)), 0.90);
        let verdict = report.control_verdict().expect("a verdict");
        assert!(!verdict.demonstrated_sensitivity());

        match verdict {
            ControlVerdict::WrongDirection { gap } => {
                assert!((gap - 0.09).abs() < 1e-9, "gap is reported positive: {gap}");
            }
            other => panic!("expected WrongDirection, got {other:?}"),
        }
    }

    /// The two failures are distinct states: one says the suite is too small
    /// or the effect too subtle, the other says the control itself is broken.
    /// They want different fixes, so they must not render the same.
    #[test]
    fn a_small_gap_and_a_wrong_direction_are_different_verdicts() {
        let small = report_with(Some(scores(0.5, None, 0.88)), 0.90);
        let wrong = report_with(Some(scores(0.5, None, 0.99)), 0.90);

        assert!(matches!(
            small.control_verdict(),
            Some(ControlVerdict::TooSmall { .. })
        ));
        assert!(matches!(
            wrong.control_verdict(),
            Some(ControlVerdict::WrongDirection { .. })
        ));
        assert_ne!(small.control_verdict(), wrong.control_verdict());
    }

    /// Both failure gaps are reported as positive magnitudes, so neither
    /// renders with a sign that has to be interpreted.
    #[test]
    fn every_verdict_reports_a_positive_gap() {
        for report in [
            report_with(Some(scores(0.2, None, 0.30)), 0.90),
            report_with(Some(scores(0.5, None, 0.88)), 0.90),
            report_with(Some(scores(0.5, None, 0.99)), 0.90),
        ] {
            let gap = match report.control_verdict().expect("a verdict") {
                ControlVerdict::Moved { gap }
                | ControlVerdict::TooSmall { gap }
                | ControlVerdict::WrongDirection { gap } => gap,
            };
            assert!(gap >= 0.0, "{gap}");
        }
    }

    /// **The control must disable truncation, not only raise the temperature.**
    /// llama.cpp runs the truncation samplers first, so a `top_k` left in force
    /// absorbs the temperature — measured on Qwen3.5-4B, where a
    /// temperature-only control scored *above* both real arms.
    #[test]
    fn the_control_disables_every_truncation_sampler() {
        let (temperature, top_k, top_p, min_p) = control_sampling();

        assert!((temperature - CONTROL_TEMPERATURE).abs() < f32::EPSILON);
        assert_eq!(top_k, 0, "top_k must be disabled, not merely widened");
        assert!(
            (top_p - 1.0).abs() < f32::EPSILON,
            "top_p keeps the nucleus"
        );
        assert!(min_p.abs() < f32::EPSILON, "min_p cuts no tail");
    }

    /// Not run is not the same as ran-and-failed — the same distinction the
    /// sampling readback draws between blind and zero divergences.
    #[test]
    fn no_control_arm_claims_nothing_either_way() {
        assert!(report_with(None, 0.90).control_verdict().is_none());
    }

    // =========================================================================
    // Runs that measured nothing
    // =========================================================================

    /// **The state that must never render as a score.** Every run failed
    /// before reaching the model, so the composite is arithmetic over 45 zeros
    /// and describes nothing.
    #[test]
    fn an_arm_where_no_run_reached_the_model_is_an_empty_column() {
        let arm = ArmScores {
            runs: 45,
            unmeasured_runs: 45,
            ..scores(0.244, None, 0.222)
        };

        assert!(arm.is_empty_column());
        assert!(
            !arm.is_partly_unmeasured(),
            "wholly empty is its own state, not a severe partial"
        );
    }

    /// A partial is a different state wanting a different action: the arm has
    /// real observations, diluted by a knowable number of empty ones.
    #[test]
    fn an_arm_with_some_failures_is_partly_unmeasured() {
        let arm = ArmScores {
            runs: 45,
            unmeasured_runs: 7,
            ..scores(0.5, None, 0.5)
        };

        assert!(arm.is_partly_unmeasured());
        assert!(!arm.is_empty_column());
    }

    /// The ordinary case must trip neither check, or the warning becomes noise
    /// and stops being read.
    #[test]
    fn a_fully_measured_arm_is_neither() {
        let arm = ArmScores {
            runs: 45,
            unmeasured_runs: 0,
            ..scores(0.9, None, 0.9)
        };

        assert!(!arm.is_empty_column());
        assert!(!arm.is_partly_unmeasured());
    }

    /// An arm that ran nothing at all has no runs to be empty *of*, and must
    /// not be reported as a failure of the upstream.
    #[test]
    fn an_arm_with_no_runs_is_not_an_empty_column() {
        let arm = ArmScores {
            runs: 0,
            unmeasured_runs: 0,
            ..scores(0.0, None, 0.0)
        };

        assert!(!arm.is_empty_column());
    }

    /// A stored report from before the field existed must read as "every run
    /// was measured", which is what it meant.
    #[test]
    fn a_legacy_arm_reads_as_fully_measured() {
        let json = r#"{
            "tool_accuracy": 0.5, "task_completion": 0.5, "composite": 0.5, "runs": 9
        }"#;
        let arm: ArmScores = serde_json::from_str(json).expect("deserializes");

        assert_eq!(arm.unmeasured_runs, 0);
        assert!(!arm.is_empty_column());
    }

    // =========================================================================
    // The A/A arm
    // =========================================================================

    /// The design of the arm in one assertion: replaying the same seeds would
    /// measure decode determinism, not the seed-draw variance that actually
    /// limits the primary comparison.
    #[test]
    fn the_replicate_seeds_are_disjoint_from_the_primary_ones() {
        let replicate = replicate_seeds(&DEFAULT_SEEDS);

        assert_eq!(replicate.len(), DEFAULT_SEEDS.len());
        for seed in &DEFAULT_SEEDS {
            assert!(
                !replicate.contains(seed),
                "seed {seed} was reused, so the A/A arm would measure nothing"
            );
        }
    }

    /// Derived, not drawn: a noise floor that changed every run could not be
    /// compared against the run before it.
    #[test]
    fn the_replicate_seeds_are_reproducible() {
        assert_eq!(
            replicate_seeds(&DEFAULT_SEEDS),
            replicate_seeds(&DEFAULT_SEEDS)
        );
        const { assert!(REPLICATE_SEED_OFFSET != 0) };
    }

    /// The noise floor is a distance, so which arm scored higher is irrelevant
    /// to it — an A/A arm that came out *ahead* of raw is drift just the same.
    #[test]
    fn the_noise_floor_is_a_distance_not_a_direction() {
        let above = report_with_replicate(0.20, 0.05);
        let below = report_with_replicate(0.20, -0.05);

        assert!((above.noise_floor().unwrap() - 0.05).abs() < 1e-9);
        assert!((below.noise_floor().unwrap() - 0.05).abs() < 1e-9);
    }

    /// **What the arm exists for.** An effect the same size as the eval's own
    /// drift must not be reported as a finding.
    #[test]
    fn an_effect_no_bigger_than_the_drift_is_within_noise() {
        let report = report_with_replicate(0.04, 0.03);

        let verdict = report.effect_verdict().expect("the A/A arm ran");
        assert!(!verdict.exceeds_noise());
        assert!((verdict.ratio().unwrap() - 4.0 / 3.0).abs() < 1e-9);
    }

    /// And an effect several times the drift must clear it, or the arm would
    /// veto every result it was added to qualify.
    #[test]
    fn an_effect_well_past_the_drift_clears_it() {
        let report = report_with_replicate(0.30, 0.03);

        let verdict = report.effect_verdict().expect("the A/A arm ran");
        assert!(verdict.exceeds_noise());
        assert!((verdict.ratio().unwrap() - 10.0).abs() < 1e-9);
    }

    /// A negative effect that clears the drift is still a resolved measurement.
    /// Reporting only favourable findings as real is the failure mode an A/A
    /// arm is supposed to prevent, not introduce.
    #[test]
    fn a_negative_effect_can_also_exceed_the_noise() {
        let verdict = report_with_replicate(-0.30, 0.03)
            .effect_verdict()
            .expect("the A/A arm ran");

        assert!(verdict.exceeds_noise());
        assert!(verdict.effect() < 0.0, "the sign survives the verdict");
    }

    /// Two arms landing on the identical composite is an unresolved drift, not
    /// an infinitely precise one, so nothing may divide by it.
    #[test]
    fn a_zero_drift_yields_no_ratio() {
        let report = report_with_replicate(0.08, 0.0);
        let verdict = report.effect_verdict().expect("the A/A arm ran");

        assert_eq!(verdict.ratio(), None);
        assert!(
            verdict.exceeds_noise(),
            "a real effect over no measured drift"
        );
    }

    /// Both terms zero is the vacuous case: no effect, no drift, and nothing
    /// that may be reported as having exceeded anything.
    #[test]
    fn no_effect_over_no_drift_is_not_a_finding() {
        let report = report_with_replicate(0.0, 0.0);

        assert!(!report.effect_verdict().expect("ran").exceeds_noise());
    }

    /// Without the arm there is no basis for the judgement, and the report
    /// must decline to make it rather than assume a floor of zero.
    #[test]
    fn no_replicate_arm_yields_no_effect_verdict() {
        let report = report_with(None, 0.90);

        assert_eq!(report.noise_floor(), None);
        assert!(report.effect_verdict().is_none());
    }

    /// The threshold has to be above 1.0: an effect merely *equal* to the
    /// drift is exactly the case the arm was added to catch.
    #[test]
    fn the_noise_ratio_demands_more_than_parity() {
        const { assert!(EFFECT_NOISE_RATIO > 1.0) };
        assert!(
            !report_with_replicate(0.05, 0.05)
                .effect_verdict()
                .expect("ran")
                .exceeds_noise()
        );
    }

    /// A stored report from before the A/A arm existed must read as "no
    /// replicate ran" rather than failing to deserialize.
    #[test]
    fn a_legacy_report_has_no_replicate_arm() {
        let json = r#"{
            "model_name": "m", "quantization": null, "param_count_b": 1.0,
            "ctx_size": 4096,
            "raw": {"tool_accuracy": 0.5, "task_completion": 0.5, "composite": 0.5},
            "gglib": {"tool_accuracy": 0.9, "task_completion": 0.9, "composite": 0.9},
            "delta": {"tool_accuracy": 0.4, "task_completion": 0.4, "composite": 0.4},
            "tasks": []
        }"#;
        let report: AgenticEvalReport = serde_json::from_str(json).expect("deserializes");

        assert!(report.raw_replicate.is_none());
        assert!(report.replicate_seeds.is_empty());
        assert!(report.effect_verdict().is_none());
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
        let verdict = exactly_at.control_verdict();
        assert!(
            matches!(verdict, Some(ControlVerdict::Moved { .. })),
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

    // ── PairedEffect ─────────────────────────────────────────────────────────

    /// A result with a chosen match score, for paired fixtures.
    fn scored_result(id: &str, score: f64) -> TuneTaskResult {
        TuneTaskResult {
            tool_match_score: score,
            passed: score >= 1.0,
            ..task_result(id, false)
        }
    }

    fn paired_task(id: &str, raw_scores: &[f64], gglib_scores: &[f64]) -> AgenticTaskComparison {
        AgenticTaskComparison {
            task_id: id.to_owned(),
            category: TaskCategory::SingleCall,
            raw: raw_scores.iter().map(|s| scored_result(id, *s)).collect(),
            gglib: gglib_scores.iter().map(|s| scored_result(id, *s)).collect(),
        }
    }

    #[test]
    fn paired_effect_counts_wins_losses_ties_and_means_the_deltas() {
        let tasks = vec![
            paired_task("a", &[0.5, 1.0], &[1.0, 1.0]), // one win, one tie
            paired_task("b", &[1.0], &[0.5]),           // one loss
        ];
        let paired = PairedEffect::from_tasks(&tasks).expect("pairs exist");
        assert_eq!(paired.pairs, 3);
        assert_eq!((paired.wins, paired.losses, paired.ties), (1, 1, 1));
        assert!((paired.mean_delta - 0.0).abs() < 1e-12, "{paired:?}");
        assert_eq!(paired.unmeasured_pairs, 0);
        // Two non-tied pairs is far below the Wilcoxon minimum.
        assert_eq!(paired.p_value, None);
    }

    /// A pair either side of which never reached the model is dropped and
    /// counted, never scored — the same rule `ArmScores::unmeasured_runs`
    /// applies arm-wide, kept at pair granularity here.
    #[test]
    fn paired_effect_drops_unmeasured_pairs_and_says_so() {
        let mut task = paired_task("a", &[0.5, 0.5], &[1.0, 1.0]);
        task.raw[1].unmeasured = Some("upstream unreachable".to_owned());
        let paired = PairedEffect::from_tasks(&[task]).expect("one live pair");
        assert_eq!(paired.pairs, 1);
        assert_eq!(paired.unmeasured_pairs, 1);
        assert_eq!(paired.wins, 1);
    }

    #[test]
    fn paired_effect_is_none_when_nothing_was_measured() {
        assert!(PairedEffect::from_tasks(&[]).is_none());
        let mut task = paired_task("a", &[0.5], &[1.0]);
        task.gglib[0].unmeasured = Some("dead".to_owned());
        assert!(PairedEffect::from_tasks(&[task]).is_none());
    }

    /// Ten distinct all-positive deltas: W⁻ = 0, and the normal approximation
    /// with continuity correction gives z = (0 − 27.5 + 0.5)/√96.25 ≈ −2.752,
    /// p ≈ 0.0030. Pinned inside a band an implementation error of one rank,
    /// one correction term, or a dropped tail would leave.
    #[test]
    fn wilcoxon_all_positive_deltas_is_a_strong_result() {
        let gglib: Vec<f64> = (1..=10).map(|i| f64::from(i) * 0.05).collect();
        let raw = vec![0.0; 10];
        let tasks = vec![paired_task("a", &raw, &gglib)];
        let p = PairedEffect::from_tasks(&tasks)
            .unwrap()
            .p_value
            .expect("ten non-tied pairs");
        assert!(p > 0.001 && p < 0.005, "p = {p}");
    }

    /// Symmetric wins and losses of matching magnitude: W⁻ lands on its null
    /// mean and the one-sided p sits at chance.
    #[test]
    fn wilcoxon_balanced_deltas_read_as_chance() {
        let raw = vec![0.5; 10];
        let gglib = vec![0.6, 0.4, 0.7, 0.3, 0.8, 0.2, 0.9, 0.1, 1.0, 0.0];
        let tasks = vec![paired_task("a", &raw, &gglib)];
        let p = PairedEffect::from_tasks(&tasks)
            .unwrap()
            .p_value
            .expect("ten non-tied pairs");
        assert!(p > 0.4 && p < 0.6, "p = {p}");
    }

    /// Below the minimum the statistic says nothing — ties do not count
    /// toward the minimum, because zeros are dropped before ranking.
    #[test]
    fn wilcoxon_says_nothing_below_the_minimum() {
        let raw = vec![0.5; 10];
        let mut gglib = vec![0.5; 10]; // ties everywhere...
        for (i, value) in gglib.iter_mut().enumerate().take(7) {
            *value = 0.01f64.mul_add(f64::from(u8::try_from(i).unwrap()), 0.6);
        }
        let tasks = vec![paired_task("a", &raw, &gglib)];
        let paired = PairedEffect::from_tasks(&tasks).unwrap();
        assert_eq!(paired.pairs, 10);
        assert_eq!(paired.wins, 7);
        assert_eq!(paired.p_value, None);
    }

    // ── Multi-pair A/A drift ────────────────────────────────────────────────

    /// Three runs of the identical configuration give three pairwise gaps,
    /// and the floor is their mean — more degrees of freedom, same estimator.
    #[test]
    fn noise_floor_over_multiple_pairs_averages_all_pairwise_gaps() {
        let mut report = report_with(None, 0.9);
        report.raw = scores(0.5, None, 0.7);
        report.raw_replicates = vec![scores(0.5, None, 0.6), scores(0.5, None, 0.8)];
        // gaps: |0.7−0.6| = 0.1, |0.7−0.8| = 0.1, |0.6−0.8| = 0.2
        let floor = report.noise_floor().expect("replicates ran");
        assert!((floor - 0.4 / 3.0).abs() < 1e-12, "{floor}");
        assert_eq!(report.noise_pairs(), 3);
        assert_eq!(report.effect_verdict().expect("has drift").pairs(), 3);
    }

    /// A report written before the multi-pair field existed — populated
    /// `raw_replicate`, empty `raw_replicates` — reads exactly as it always
    /// did: one gap, one pair.
    #[test]
    fn a_legacy_single_pair_report_reads_exactly_as_before() {
        let report = report_with_replicate(0.2, 0.05);
        assert!((report.noise_floor().unwrap() - 0.05).abs() < 1e-12);
        assert_eq!(report.noise_pairs(), 1);
        assert_eq!(report.effect_verdict().unwrap().pairs(), 1);
    }

    /// Pair 1 must reproduce the single-pair seed set, or a multi-pair run's
    /// first pair would not be comparable with every run recorded before it.
    #[test]
    fn the_first_replicate_seed_set_is_the_legacy_one() {
        assert_eq!(
            replicate_seed_set(&DEFAULT_SEEDS, 1),
            replicate_seeds(&DEFAULT_SEEDS)
        );
        let second = replicate_seed_set(&DEFAULT_SEEDS, 2);
        assert_ne!(second, replicate_seeds(&DEFAULT_SEEDS));
        assert_ne!(second, DEFAULT_SEEDS.to_vec());
    }

    /// The report derives it from the drill-down it already stores, so a
    /// legacy report gains the paired view retroactively.
    #[test]
    fn a_report_derives_its_paired_effect_from_tasks() {
        let mut report = report_with(None, 0.7);
        assert!(report.paired_effect().is_none(), "no drill-down stored");
        report.tasks = vec![paired_task("a", &[0.5], &[1.0])];
        let paired = report.paired_effect().expect("one pair");
        assert_eq!((paired.pairs, paired.wins), (1, 1));
    }
}
