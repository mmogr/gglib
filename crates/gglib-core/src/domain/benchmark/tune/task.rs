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

impl TaskSuite {
    /// Embedded JSON for the built-in default suite (BFCL-style: single-call,
    /// parallel-call, multi-turn, and irrelevance-detection scenarios).
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
mod tests {
    use super::*;

    /// `ExpectedOutcome` is `#[serde(tag = "kind")]` (internally tagged), which
    /// only supports newtype variants whose inner value serializes as a JSON
    /// object/map. `ToolCalls` must therefore stay a *struct* variant
    /// (`{ calls: Vec<..> }`), never a bare `ToolCalls(Vec<..>)` newtype —
    /// the latter fails at serialization time with "cannot serialize tagged
    /// newtype variant ... containing a sequence".
    #[test]
    fn expected_outcome_tool_calls_round_trips() {
        let outcome = ExpectedOutcome::ToolCalls {
            calls: vec![ExpectedCall {
                name: "get_weather".to_string(),
                required_args: serde_json::Map::new(),
                ordered: false,
            }],
        };
        let json = serde_json::to_string(&outcome).expect("serializes");
        let round_tripped: ExpectedOutcome =
            serde_json::from_str(&json).expect("deserializes");
        assert!(matches!(round_tripped, ExpectedOutcome::ToolCalls { .. }));
    }

    #[test]
    fn expected_outcome_no_tool_call_round_trips() {
        let json = serde_json::to_string(&ExpectedOutcome::NoToolCall).expect("serializes");
        let round_tripped: ExpectedOutcome =
            serde_json::from_str(&json).expect("deserializes");
        assert!(matches!(round_tripped, ExpectedOutcome::NoToolCall));
    }

    #[test]
    fn task_suite_custom_round_trips() {
        let suite = TaskSuite::Custom {
            tasks: vec![TuneTask {
                id: "single_call_example".to_string(),
                category: TaskCategory::SingleCall,
                system_prompt: None,
                user_prompt: "What's the weather in Boston?".to_string(),
                tools: vec![],
                expected: ExpectedOutcome::NoToolCall,
            }],
        };
        let json = serde_json::to_string(&suite).expect("serializes");
        let round_tripped: TaskSuite = serde_json::from_str(&json).expect("deserializes");
        assert!(matches!(round_tripped, TaskSuite::Custom { .. }));
    }

    /// Guards the embedded default suite asset: it must always parse, and
    /// must cover all four BFCL-style categories so the pre-screen round
    /// (which picks one `SingleCall` + one `Irrelevance` task) always has
    /// candidates to draw from.
    #[test]
    fn default_suite_parses_and_covers_all_categories() {
        let tasks = TaskSuite::Default.resolve().expect("embedded suite parses");
        assert!(!tasks.is_empty(), "default suite must not be empty");

        for category in [
            TaskCategory::SingleCall,
            TaskCategory::ParallelCall,
            TaskCategory::MultiTurn,
            TaskCategory::Irrelevance,
        ] {
            assert!(
                tasks.iter().any(|t| t.category == category),
                "default suite missing a task in category {category:?}"
            );
        }

        // Task IDs must be unique — the tune service keys results by ID.
        let mut ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), tasks.len(), "default suite has duplicate task IDs");
    }
}
