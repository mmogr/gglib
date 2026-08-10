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
    /// Whether to run the A/A arm. See [`EvalArm::RawReplicate`].
    ///
    /// On by default, and cheap: the raw arm is the fastest of the three, so
    /// repeating it costs a fraction of what the control does and is the only
    /// thing in the report that speaks to the *size* of an effect rather than
    /// its direction.
    #[serde(default = "default_replicate_raw")]
    pub replicate_raw: bool,
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
    seeds
        .iter()
        .map(|seed| seed.wrapping_add(REPLICATE_SEED_OFFSET))
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
        /// `|raw − raw_replicate|`, the drift between two identical arms.
        noise: f64,
    },
    /// The effect is not clearly larger than the drift between two runs of the
    /// same arm. It is not thereby *absent* — it is unresolved at this seed
    /// count, and the fix is more seeds rather than a different conclusion.
    WithinNoise {
        /// `gglib − raw`, signed.
        effect: f64,
        /// `|raw − raw_replicate|`, the drift between two identical arms.
        noise: f64,
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
            Self::ExceedsNoise { effect, noise } | Self::WithinNoise { effect, noise } => {
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

    /// Whether the control demonstrated sensitivity. `None` when it did not
    /// run.
    #[must_use]
    pub fn control_moved(&self) -> Option<bool> {
        self.control_verdict().map(|v| v.demonstrated_sensitivity())
    }

    /// The eval's own drift: how far two identical raw arms landed apart.
    ///
    /// `None` when the A/A arm did not run, which is distinct from a measured
    /// zero for the same reason `Blind` is distinct from zero divergences.
    #[must_use]
    pub fn noise_floor(&self) -> Option<f64> {
        let replicate = self.raw_replicate.as_ref()?;
        Some((self.raw.composite - replicate.composite).abs())
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
        Some(
            if effect.abs() > 0.0 && effect.abs() >= EFFECT_NOISE_RATIO * noise {
                EffectVerdict::ExceedsNoise { effect, noise }
            } else {
                EffectVerdict::WithinNoise { effect, noise }
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
            raw_replicate: None,
            replicate_seeds: vec![],
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

    /// **The failure that was measured, and that the old bool hid.** A control
    /// scoring *above* the gglib arm contradicts its premise rather than
    /// failing a threshold, and it must not be reported as "barely moved".
    #[test]
    fn a_control_that_scored_higher_is_its_own_verdict() {
        let report = report_with(Some(scores(0.95, None, 0.99)), 0.90);

        match report.control_verdict() {
            Some(ControlVerdict::WrongDirection { gap }) => {
                assert!((gap - 0.09).abs() < 1e-9, "gap is reported positive: {gap}");
            }
            other => panic!("expected WrongDirection, got {other:?}"),
        }
        assert_eq!(report.control_moved(), Some(false));
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
        assert_eq!(report_with(None, 0.90).control_moved(), None);
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
