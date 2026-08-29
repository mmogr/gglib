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

/// The *shape* of what a run generated, as opposed to how much.
///
/// # Why this exists
///
/// Until this struct, the eval counted output and threw it away: 7 of the 9
/// `AgentEvent` variants — `TextDelta` and `ReasoningDelta` among them — fell
/// through the benchmark's event loop untouched. A run was therefore knowable
/// only as a token total and a wall time.
///
/// That is not enough to read a run. On 2026-08-29 five runs generated ~32,900
/// completion tokens apiece against ~510 for the same task without the
/// pipeline, took ~950s, and **passed**. Nothing recorded anywhere could say
/// whether that was a small reasoning model thinking at length or a generation
/// fault, and the two call for opposite responses. This struct is the
/// difference between those two readings.
///
/// Every field is taken from events the loop already emitted, so nothing here
/// changes what the eval sends, executes or scores.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct GeneratedOutput {
    /// Characters the model emitted as **reasoning** (chain-of-thought).
    ///
    /// # This is only meaningful when the upstream separates reasoning
    ///
    /// It counts `AgentEvent::ReasoningDelta`, which exists only when
    /// llama-server was launched with `--reasoning-format deepseek` and so
    /// splits thinking into its own `reasoning_content` SSE field. Without that
    /// flag a reasoning model's thinking arrives inline as `<think>…</think>`,
    /// the normalizer strips the tags, and every one of those characters is
    /// counted as [`Self::answer_chars`] instead.
    ///
    /// So `reasoning_chars: 0` beside a large `answer_chars` is **ambiguous**:
    /// it means either the model did not think, or it thought and nobody could
    /// tell. Resolve it by checking whether the model carries the `reasoning`
    /// capability tag, not by assuming.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub reasoning_chars: u64,
    /// Characters the model emitted as ordinary answer text, summed across
    /// every turn — not just the final one.
    ///
    /// Counted from `AgentEvent::TextDelta` rather than from `FinalAnswer`,
    /// which carries the same text already accumulated and would double it.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub answer_chars: u64,
    /// How many requests the run actually sent to the model.
    ///
    /// Distinct from [`TuneTaskResult::iterations`], which counts only
    /// *tool-executing* turns — a run that ends by answering in text made one
    /// more request than it reports iterations. Dividing tokens by `iterations`
    /// therefore overstates per-request generation, by 50% on a two-iteration
    /// run, which is exactly the arithmetic a reader performs when asking
    /// whether a token cap was in force.
    ///
    /// Derived from the event stream (one per `IterationComplete`, plus one for
    /// a `FinalAnswer`), so a run a guard aborted mid-turn under-counts by the
    /// aborting request. Read it as a floor on those runs.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub llm_calls: usize,
    /// The largest single batch of tool calls any one turn executed.
    ///
    /// The fingerprint of a constrained-decoding runaway. gglib's generated
    /// grammar admits `root ::= sp call (sp call)* sp` — unbounded repetition —
    /// so a model that never emits an end-of-generation token can keep producing
    /// syntactically valid calls until it hits a token cap or the context limit.
    /// Scoring cannot reveal this: extra unrequested calls cost nothing, so a
    /// batch of hundreds containing the right call still scores `1.0` and the
    /// task still reads as passed.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub max_tool_calls_in_batch: usize,
    /// How many recoverable conditions the loop reported during this run.
    ///
    /// Counts `AgentEvent::SystemWarning`, whose main source is the loop
    /// recovering from a model that requested more parallel tool calls than the
    /// configured limit. That recovery costs a whole extra request and was
    /// previously invisible to the eval: the warning was emitted, discarded, and
    /// the run reported as though nothing had happened.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub system_warnings: u32,
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
    /// How many attempts this run threw away to a transport failure before the
    /// one reported here.
    ///
    /// `0` on a run that succeeded first time. Non-zero means the harness hit
    /// [`Self::unmeasured`] and tried again, so the numbers above come from a
    /// later attempt than the one the suite nominally ran.
    ///
    /// Recorded rather than swallowed because a silently-retried run is not the
    /// same measurement as a clean one, and an eval that hides its retries can
    /// report a healthy suite while the upstream underneath it is failing one
    /// request in ten. It is also the reading its own kill criterion needs: if
    /// this stays `0` across two full evals, the retry is unnecessary and goes.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub transport_retries: u32,
    /// What the model generated, as opposed to how much of it.
    ///
    /// See [`GeneratedOutput`] — a token total and a wall time cannot
    /// distinguish a model thinking at length from one failing to stop, and
    /// those call for opposite responses.
    #[serde(default)]
    pub generated: GeneratedOutput,
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

#[cfg(test)]
#[path = "result_tests.rs"]
mod result_tests;
