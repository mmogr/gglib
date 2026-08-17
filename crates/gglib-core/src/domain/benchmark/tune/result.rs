//! Per-candidate and per-task results produced by a tune run.

use serde::{Deserialize, Serialize};

use crate::domain::inference::InferenceConfig;

use super::task::TaskCategory;

/// Where a tune candidate's sampling settings came from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum CandidateSource {
    /// One point in the user-specified [`super::config::SweepSpec`] grid.
    UserGrid,
    /// Seeded from the built-in per-model-family preset table (e.g. Qwen
    /// coding-mode defaults).
    FamilyPreset {
        /// Display name of the matched family/preset (e.g. `"qwen-coding"`).
        family: String,
    },
    /// The model's current behaviour: an all-`None` overlay, which resolves
    /// through the normal chain and is therefore exactly what an untouched
    /// request gets today. Always included, never pruned — a winner that
    /// never raced the incumbent has not beaten it.
    Incumbent,
    /// The incumbent again, identically. The gap between the twins is the
    /// run's own drift — the in-run calibration the apply gate divides every
    /// margin by (see `tune::apply`). Excluded from the leaderboard's notion
    /// of "winner": it is an instrument, not a contender.
    IncumbentCalibration,
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
        assert!(matches!(
            round_tripped,
            CandidateSource::FamilyPreset { .. }
        ));
    }

    /// `UserGrid` is the only unit variant left since `GgufAuthorDefault` was
    /// deleted, so this is a single case rather than the loop it used to be.
    #[test]
    fn candidate_source_unit_variants_round_trip() {
        let json = serde_json::to_string(&CandidateSource::UserGrid).expect("serializes");
        let round_tripped: CandidateSource = serde_json::from_str(&json).expect("deserializes");
        assert!(matches!(round_tripped, CandidateSource::UserGrid));
    }
}

/// Result of evaluating one task against one candidate's sampling settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    /// Number of *tool-executing* agent-loop iterations that completed.
    ///
    /// The loop reports an iteration only after it has executed that turn's
    /// tool calls, so a turn that answered in text — including the final one —
    /// is not counted, and a guard-aborted run reports one fewer than the turn
    /// it aborted on. Read it as "how many tool-call batches this run
    /// produced", which is what decides whether a repeat was even possible.
    pub iterations: usize,
    /// Wall-clock time spent on this task, in milliseconds.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub latency_ms: u64,
    /// Completion tokens generated across the task's agent run, summed from
    /// the upstream's per-response usage reports.
    ///
    /// Counted independently of how the run ended, so a run a guard aborted
    /// still reports the tokens it burned — those are the runs whose cost
    /// matters most. `None` only when the upstream reported no usage at all,
    /// which stays distinct from a measured zero.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub completion_tokens: Option<u64>,
    /// Wall-clock milliseconds from the start of the task to the first tool
    /// call the model actually issued — how long it took to take its first
    /// useful action.
    ///
    /// This is the figure an agentic client's user feels: a turn that emits a
    /// valid call in 300 ms and one that emits the same call after 140 s of
    /// unconstrained generation score identically on every accuracy axis.
    /// `None` when the task never called a tool, which is the correct outcome
    /// for an `Irrelevance` task.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub time_to_first_tool_call_ms: Option<u64>,
    /// Optional human-readable detail (e.g. which expected call was missed),
    /// surfaced in the leaderboard drill-down.
    #[serde(default)]
    pub detail: Option<String>,
    /// Why this run is **not a measurement of the model**, when it is not one.
    ///
    /// `None` on every run that actually reached the model, including every
    /// way of doing badly: a wrong tool call, a detected loop, a stagnated
    /// answer and an exhausted iteration budget are all real observations and
    /// score honestly as failures.
    ///
    /// `Some(reason)` is the different thing — the request never produced a
    /// response to score, because the upstream was unreachable, the stream
    /// broke, or the loop could not start. Such a run still carries
    /// `passed: false` and `tool_match_score: 0.0`, and **those zeros mean
    /// nothing**: they are the absence of a measurement wearing the costume of
    /// a bad one.
    ///
    /// Measured, which is why this field exists. A run whose llama-server had
    /// died scored a composite of `0.222` across 45 failed requests and
    /// rendered as an ordinary, believable arm — a −0.562 delta that read as a
    /// catastrophic regression rather than as an empty column. An arm that
    /// cannot tell "the model did badly" from "there was no model" is
    /// reporting a number it never took.
    #[serde(default)]
    pub unmeasured: Option<String>,
}

impl TuneTaskResult {
    /// Whether this run produced a real observation of the model.
    ///
    /// Read this rather than `passed`, wherever the question is "is this
    /// number worth anything" rather than "did the model succeed".
    #[must_use]
    pub const fn is_measured(&self) -> bool {
        self.unmeasured.is_none()
    }
}

/// Result of evaluating one candidate's sampling settings against the full
/// (or pre-screen) task suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
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
    /// Completion-token throughput observed for this candidate, in tokens
    /// per wall-clock second across its evaluated tasks (total completion
    /// tokens ÷ total task wall time, which includes prompt pre-fill — a
    /// consistent within-run comparison figure, not a pure decode rate).
    /// `None` when no task reported usage.
    #[serde(default)]
    pub tg_tps: Option<f64>,
}
