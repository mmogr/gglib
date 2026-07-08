//! SSE / channel event enum shared by all benchmark run types.

use serde::{Deserialize, Serialize};

use super::compare::ModelCompareResult;
use super::perf::ModelPerfResult;
use super::tune::TuneCandidateResult;

/// Typed event emitted over the mpsc channel (and serialised as SSE to the
/// browser) during a benchmark run.
///
/// Both the CLI renderer and the Axum SSE bridge consume this enum, giving
/// full feature parity between CLI and web interfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BenchmarkEvent {
    /// A model is about to start (position is 1-based).
    ModelStarted {
        model_id: i64,
        model_name: String,
        position: usize,
        total: usize,
    },
    /// A chunk of generated text for one model (compare runs only).
    ModelTextDelta { model_id: i64, text: String },
    /// A model finished successfully.
    ModelComplete {
        model_id: i64,
        result: BenchmarkModelResult,
    },
    /// A model failed (e.g. binary not found, OOM).
    ModelFailed {
        model_id: i64,
        model_name: String,
        error: String,
    },
    /// All models finished; the run record is now `Complete`.
    RunComplete { run_id: i64 },
    /// The entire run failed (e.g. DB error, abort).
    RunFailed { error: String },

    /// A tune candidate is about to be evaluated (index is 0-based).
    TuneCandidateStarted {
        candidate_index: usize,
        total: usize,
    },
    /// One task finished evaluating for the current tune candidate.
    TuneTaskComplete {
        candidate_index: usize,
        task_id: String,
        passed: bool,
    },
    /// A tune candidate was dropped after the pre-screen round and will not
    /// run the full task suite.
    TunePruned {
        candidate_index: usize,
        reason: String,
    },
    /// A tune candidate finished evaluating (pre-screen or full suite).
    TuneCandidateComplete { result: TuneCandidateResult },
}

/// Wraps either a compare or perf result for `BenchmarkEvent::ModelComplete`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BenchmarkModelResult {
    /// Result from a compare run.
    Compare(ModelCompareResult),
    /// Result from a perf run.
    Perf(ModelPerfResult),
}
