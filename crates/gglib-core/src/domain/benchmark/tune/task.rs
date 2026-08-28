//! Agentic tool-calling task schema for tune runs.
//!
//! A [`TuneTask`] is a single scripted scenario: a prompt, a set of tools
//! advertised to the model, and an expected outcome. Tasks are evaluated
//! through the real agent loop (not a toy harness) so the recorded tool
//! calls reflect exactly what the model would do in production.
//!
//! # Categories
//!
//! Modeled after the Berkeley Function Calling Leaderboard (BFCL)
//! methodology: single-call and parallel-call correctness, multi-turn
//! (stateful) tool use, and — importantly for avoiding loops — irrelevance
//! detection (can the model correctly abstain from calling a tool when none
//! applies). A fifth category, [`TaskCategory::LongContext`], goes beyond
//! BFCL: it pre-fills the conversation with a long simulated history before
//! `user_prompt`, testing whether context degradation over a long session
//! (attention fixating on stale context) causes the model to loop or
//! stagnate on a task it would otherwise handle cleanly from a cold start.

use serde::{Deserialize, Serialize};

use crate::domain::agent::{AgentMessage, ToolDefinition};

/// Category of an agentic tool-calling scenario, following the BFCL split
/// (plus [`LongContext`](Self::LongContext), which is gglib-specific).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub enum TaskCategory {
    /// Exactly one tool call is expected.
    SingleCall,
    /// Multiple independent tool calls are expected in the same turn.
    ParallelCall,
    /// A multi-turn, stateful scenario requiring sequential tool calls
    /// that build on prior tool results.
    MultiTurn,
    /// No tool call is expected at all — tests whether the model correctly
    /// abstains instead of calling a tool it doesn't need.
    Irrelevance,
    /// Same evaluation as the other categories, but `user_prompt` is sent
    /// after [`TuneTask::history`] has already been injected into the
    /// conversation — tests whether a long prior session (thousands of
    /// tokens of simulated dummy code/turns) causes the model to lose
    /// attention and trigger the agent loop's `LoopDetector`/
    /// `StagnationDetector`, or mis-call a tool it would otherwise get
    /// right from a cold start.
    LongContext,
}

/// One expected tool call within a task's [`ExpectedOutcome::ToolCalls`].
///
/// Matching is AST-style (BFCL-inspired), not a string diff: the recorded
/// call's `name` must match exactly, and `required_args` must be a subset of
/// the recorded arguments (extra arguments the model supplies are ignored).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedCall {
    /// Expected tool name.
    pub name: String,
    /// Required argument key/value pairs. The recorded call's arguments must
    /// contain each of these keys with matching values; additional
    /// arguments in the recorded call are ignored.
    #[serde(default)]
    pub required_args: serde_json::Map<String, serde_json::Value>,
    /// When `true`, this call must occur in the given position relative to
    /// other expected calls (order matters). When `false`, expected calls
    /// may be matched against recorded calls in any order.
    ///
    /// Ordering is checked across tool-call *batches*, not across the flat
    /// call log. Two calls the model emitted in one parallel batch were not
    /// ordered by the model at all, so demanding an order between them scores
    /// a scheduler's arbitrary completion sequence rather than the model.
    #[serde(default)]
    pub ordered: bool,
    /// When `true`, this call's arguments depend on the **result** of the call
    /// before it, so it may only be credited in a strictly later batch.
    ///
    /// [`Self::ordered`] alone cannot express this. A model that emits
    /// `file_exists` and `delete_file` in a single parallel batch satisfies
    /// every ordering constraint available — the calls are simultaneous, so no
    /// order is violated — while demonstrating none of the competency the task
    /// exists to test: it deleted the file without ever seeing whether it was
    /// there. Marking the second call here makes the two-turn structure a
    /// requirement rather than an accident of how the model chose to batch.
    ///
    /// Deliberately **not** set on every multi-turn task. Creating a file and
    /// then appending to a path you already know needs no intervening result,
    /// so a model that does both at once is being more efficient rather than
    /// skipping a step, and should keep the credit.
    #[serde(default)]
    pub depends_on_result: bool,
}

/// What a task expects the agent loop to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpectedOutcome {
    /// One or more tool calls are expected, matched AST-style.
    ToolCalls {
        /// The expected calls (order-checked only when a call sets `ordered: true`).
        calls: Vec<ExpectedCall>,
    },
    /// No tool call is expected (irrelevance-detection task).
    NoToolCall,
}

/// A single scripted agentic scenario evaluated during a tune run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuneTask {
    /// Stable identifier for this task (used in results and diagnostics).
    pub id: String,
    /// BFCL-style category this task belongs to.
    pub category: TaskCategory,
    /// Optional system prompt for this task.
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Simulated prior conversation turns injected before `user_prompt`,
    /// used by [`TaskCategory::LongContext`] tasks to test whether context
    /// degradation over a long session induces loop/stagnation behavior
    /// that would not occur from a cold start. `None`/empty for every other
    /// category.
    #[serde(default)]
    pub history: Option<Vec<AgentMessage>>,
    /// User prompt sent to the agent loop (after `history`, if present).
    pub user_prompt: String,
    /// Tools advertised to the model for this task (OpenAI-format schema).
    pub tools: Vec<ToolDefinition>,
    /// Expected outcome used to score the recorded tool calls.
    pub expected: ExpectedOutcome,
}

/// The set of tasks a tune run evaluates each candidate against.
///
/// `Custom` carries the exact same JSON shape whether it originates from a
/// CLI `--task-suite path.json` file or a GUI file upload parsed
/// client-side and posted as part of the run request — there is a single
/// shared schema, not two divergent ingestion paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum TaskSuite {
    /// The built-in default suite (see `assets/tune_default_suite.json`).
    Default,
    /// A user-authored suite.
    Custom { tasks: Vec<TuneTask> },
}

impl TaskSuite {
    /// Embedded JSON for the built-in default suite (BFCL-style: single-call,
    /// parallel-call, multi-turn, and irrelevance-detection scenarios, plus
    /// a long-context endurance scenario).
    const DEFAULT_SUITE_JSON: &'static str =
        include_str!("../../../../assets/tune_default_suite.json");

    /// Resolve this suite into its concrete list of tasks.
    ///
    /// # Errors
    ///
    /// Returns an error only for [`TaskSuite::Default`], and only if the
    /// embedded JSON asset is malformed — that would indicate a build-time
    /// bug in gglib itself, never a user input error. [`TaskSuite::Custom`]
    /// never errors here (its tasks were already deserialized when the
    /// `TaskSuite` value itself was parsed).
    pub fn resolve(&self) -> Result<Vec<TuneTask>, serde_json::Error> {
        match self {
            Self::Default => serde_json::from_str(Self::DEFAULT_SUITE_JSON),
            Self::Custom { tasks } => Ok(tasks.clone()),
        }
    }
}

#[cfg(test)]
#[path = "task_tests.rs"]
mod task_tests;
