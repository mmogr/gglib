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
//! applies).

use serde::{Deserialize, Serialize};

use crate::domain::agent::ToolDefinition;

/// Category of an agentic tool-calling scenario, following the BFCL split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    #[serde(default)]
    pub ordered: bool,
}

/// What a task expects the agent loop to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExpectedOutcome {
    /// One or more tool calls are expected, matched AST-style.
    ToolCalls(Vec<ExpectedCall>),
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
    /// User prompt sent to the agent loop.
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
