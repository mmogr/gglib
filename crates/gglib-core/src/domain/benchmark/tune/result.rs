//! Per-candidate and per-task results produced by a tune run.

use serde::{Deserialize, Serialize};

use crate::domain::inference::InferenceConfig;

use super::task::TaskCategory;

/// Where a tune candidate's sampling settings came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CandidateSource {
    /// One point in the user-specified [`super::config::SweepSpec`] grid.
    UserGrid,
    /// Seeded from the model's GGUF metadata author-recommended sampling
    /// defaults, when present.
    GgufAuthorDefault,
    /// Seeded from the built-in per-model-family preset table (e.g. Qwen
    /// coding-mode defaults).
    FamilyPreset {
        /// Display name of the matched family/preset (e.g. `"qwen-coding"`).
        family: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CandidateSource` is `#[serde(tag = "kind")]` (internally tagged),
    /// which only supports newtype variants whose inner value serializes as
    /// a JSON object/map. `FamilyPreset` must therefore stay a *struct*
    /// variant (`{ family: String }`), never a bare `FamilyPreset(String)`
    /// newtype — the latter fails at serialization time with "cannot
    /// serialize tagged newtype variant ... containing a string".
    #[test]
    fn candidate_source_family_preset_round_trips() {
        let source = CandidateSource::FamilyPreset {
            family: "qwen-coding".to_string(),
        };
        let json = serde_json::to_string(&source).expect("serializes");
        let round_tripped: CandidateSource = serde_json::from_str(&json).expect("deserializes");
        assert!(matches!(round_tripped, CandidateSource::FamilyPreset { .. }));
    }

    #[test]
    fn candidate_source_unit_variants_round_trip() {
        for source in [CandidateSource::UserGrid, CandidateSource::GgufAuthorDefault] {
            let json = serde_json::to_string(&source).expect("serializes");
            let _: CandidateSource = serde_json::from_str(&json).expect("deserializes");
        }
    }
}

/// Result of evaluating one task against one candidate's sampling settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuneTaskResult {
    /// ID of the [`super::task::TuneTask`] this result corresponds to.
    pub task_id: String,
    /// Category the task belongs to (carried for leaderboard grouping).
    pub category: TaskCategory,
    /// `true` if the agent loop completed and its tool calls matched the
    /// task's expected outcome (for `NoToolCall` tasks: no call was made).
    pub passed: bool,
    /// AST-style match score against the expected outcome, `0.0`–`1.0`.
    ///
    /// Partial credit: e.g. right tool name but a missing required
    /// argument scores between `0.0` and `1.0`, not a hard fail.
    pub tool_match_score: f64,
    /// `true` if the agent loop's `LoopDetector` fired during this task.
    pub loop_detected: bool,
    /// `true` if the agent loop's `StagnationDetector` fired during this task.
    pub stagnation_detected: bool,
    /// Number of agent-loop iterations consumed before completion (or
    /// before the loop was aborted by a guard).
    pub iterations: usize,
    /// Wall-clock time spent on this task, in milliseconds.
    pub latency_ms: u64,
    /// Optional human-readable detail (e.g. which expected call was missed),
    /// surfaced in the leaderboard drill-down.
    #[serde(default)]
    pub detail: Option<String>,
}

/// Result of evaluating one candidate's sampling settings against the full
/// (or pre-screen) task suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuneCandidateResult {
    /// The candidate's resolved sampling settings.
    pub config: InferenceConfig,
    /// Where this candidate's settings came from.
    pub source: CandidateSource,
    /// Per-task results for this candidate.
    pub task_results: Vec<TuneTaskResult>,
    /// Weighted composite score (see [`super::config::ScoreWeights`]).
    pub composite_score: f64,
    /// `true` if this candidate was dropped after the pre-screen round and
    /// never ran the full suite (`task_results` only covers the pre-screen
    /// tasks in that case).
    pub pruned: bool,
    /// Token-generation throughput observed for this candidate, if
    /// measured (used to normalize the `speed` scoring component).
    #[serde(default)]
    pub tg_tps: Option<f64>,
}
