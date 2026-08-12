//! Tune-run configuration: sweep specification and scoring weights.

use serde::{Deserialize, Serialize};

use super::task::TaskSuite;

/// Configuration for a tune benchmark run.
///
/// A tune run evaluates one model against many candidate `InferenceConfig`
/// sampling settings, scoring each candidate against an agentic tool-calling
/// task suite to find the settings that make the model both accurate at
/// tool calls and resistant to loop/stagnation guard triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuneConfig {
    /// Database ID of the model to tune.
    pub model_id: i64,
    /// Task suite to evaluate candidates against.
    pub task_suite: TaskSuite,
    /// Sampling-parameter values to sweep (cartesian product forms the grid).
    pub sweep: SweepSpec,
    /// Seed additional candidates from the model's GGUF metadata
    /// author-recommended sampling defaults, when present.
    #[serde(default = "SweepSpec::default_true")]
    pub seed_from_gguf: bool,
    /// Seed additional candidates from the built-in per-model-family preset
    /// table (e.g. Qwen coding-mode defaults).
    #[serde(default = "SweepSpec::default_true")]
    pub seed_from_family_presets: bool,
    /// Weights used to combine per-candidate metrics into a composite score.
    #[serde(default)]
    pub weights: ScoreWeights,
    /// Fraction of candidates dropped after the cheap pre-screen round.
    ///
    /// `0.5` drops the bottom half of candidates before running the full
    /// task suite on the remaining survivors. Clamped to `[0.0, 0.9]`.
    #[serde(default = "TuneConfig::default_prune_fraction")]
    pub prune_fraction: f32,
    /// Override the llama-server context window size for this run.
    #[serde(default)]
    pub ctx_size: Option<u64>,
}

impl TuneConfig {
    const fn default_prune_fraction() -> f32 {
        0.5
    }
}

/// Sampling-parameter values to sweep.
///
/// Each field is a list of candidate values for that dimension. The full
/// candidate grid is the cartesian product of all non-empty dimensions; an
/// empty list means "don't vary this dimension" (the resolved default from
/// the normal inference-config fallback chain is used instead).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SweepSpec {
    /// Candidate temperature values.
    #[serde(default)]
    pub temperature: Vec<f32>,
    /// Candidate top-p (nucleus sampling) values.
    #[serde(default)]
    pub top_p: Vec<f32>,
    /// Candidate top-k values.
    #[serde(default)]
    pub top_k: Vec<i32>,
    /// Candidate min-p values.
    #[serde(default)]
    pub min_p: Vec<f32>,
    /// Candidate repeat-penalty values.
    #[serde(default)]
    pub repeat_penalty: Vec<f32>,
    /// Candidate DRY multiplier values. `0.0` disables DRY, so a sweep of
    /// `0.0,0.4,0.8` measures "off" against two strengths in one run.
    ///
    /// Only the multiplier is a dimension. `dry_base`, `dry_allowed_length`
    /// and `dry_penalty_last_n` keep llama.cpp's defaults (1.75, 2, 64):
    /// varying all four would multiply the grid by 81 for parameters whose
    /// shipped values are already reasonable.
    #[serde(default)]
    pub dry_multiplier: Vec<f32>,
    /// Candidate dynatemp half-range values. `0.0` disables dynamic
    /// temperature, so `0.0,0.4` measures "off" against one strength — the
    /// direct comparison of a flat temperature against an entropy-adaptive
    /// band around the same base.
    #[serde(default)]
    pub dynatemp_range: Vec<f32>,
    /// Candidate dynatemp exponent values. Only meaningful in a grid that
    /// also sweeps (or fixes) a non-zero `dynatemp_range`; llama.cpp's
    /// default is 1.0.
    #[serde(default)]
    pub dynatemp_exponent: Vec<f32>,
    /// Candidate top-n-sigma values. `-1.0` disables the truncation, so
    /// `-1.0,1.0` measures "off" against the paper's lower bound in one run.
    #[serde(default)]
    pub top_n_sigma: Vec<f32>,
}

impl SweepSpec {
    const fn default_true() -> bool {
        true
    }
}

/// Weights used to combine per-candidate metrics into one composite score.
///
/// Each weight should be non-negative; the service normalizes the weighted
/// sum by the total weight, so the four values do not need to sum to `1.0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreWeights {
    /// Weight applied to the average tool-call match score (AST-style,
    /// partial credit) across all tasks in the suite.
    pub tool_accuracy: f32,
    /// Weight applied to `1 - (loop/stagnation trigger rate)`.
    pub loop_avoidance: f32,
    /// Weight applied to the fraction of tasks the agent completed
    /// (produced a final answer instead of erroring out).
    pub task_completion: f32,
    /// Weight applied to token-generation throughput, normalized against
    /// the fastest candidate in the same run.
    pub speed: f32,
}

impl Default for ScoreWeights {
    /// Prioritizes tool-call correctness and loop-avoidance over raw speed,
    /// reflecting that an agentic backend which loops or mis-calls tools is
    /// unusable regardless of how fast it streams tokens.
    fn default() -> Self {
        Self {
            tool_accuracy: 0.4,
            loop_avoidance: 0.3,
            task_completion: 0.2,
            speed: 0.1,
        }
    }
}
