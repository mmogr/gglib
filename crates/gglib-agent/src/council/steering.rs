//! Steering LLM call: produce a [`GraphDiff`] from a natural-language
//! instruction.
//!
//! [`steering_call`] wraps [`crate::structured_output::get_structured`] with
//! the steering system prompt and JSON schema so the LLM returns a single,
//! typed [`GraphDiff`] that the executor can immediately apply.
//!
//! # Usage in the executor
//!
//! At each wave boundary (depth 0 only) the executor drains the per-run
//! [`NoteQueue`], calls [`steering_call`] once per queued instruction, and
//! applies the returned diff via
//! [`TaskGraph::apply_diff`](gglib_core::domain::council::task_graph::TaskGraph::apply_diff).

use std::sync::Arc;

use gglib_core::domain::council::task_graph::{GraphDiff, TaskGraph};
use gglib_core::ports::LlmCompletionPort;
use gglib_core::{AgentMessage, StructuredOutputError};

use crate::structured_output::get_structured;

use super::prompts::{STEERING_SYSTEM_PROMPT, graph_diff_schema};

// =============================================================================
// NoteQueue
// =============================================================================

/// A shared, per-run queue of pending steering instructions.
///
/// The HTTP note endpoint appends instructions; the executor drains them at
/// each wave boundary (depth 0 only).
pub type NoteQueue = Arc<tokio::sync::Mutex<Vec<String>>>;

// =============================================================================
// SteeringError
// =============================================================================

/// Error returned by [`steering_call`].
#[derive(Debug, thiserror::Error)]
pub enum SteeringError {
    /// The structured LLM call failed or returned unparseable output.
    #[error("steering LLM call failed: {0}")]
    Llm(#[from] StructuredOutputError),
}

// =============================================================================
// steering_call
// =============================================================================

/// Ask the LLM to produce a single [`GraphDiff`] from a natural-language
/// instruction given the current task graph.
///
/// # Parameters
///
/// - `graph` — the current task graph (serialised as context).
/// - `instruction` — the user's change request in plain language.
/// - `llm` — the LLM completion port to use.
///
/// # Retries
///
/// Delegates to [`get_structured`] with 2 retries on parse failure.
pub async fn steering_call(
    graph: &TaskGraph,
    instruction: &str,
    llm: &Arc<dyn LlmCompletionPort>,
) -> Result<GraphDiff, SteeringError> {
    let graph_json = serde_json::to_string_pretty(graph).unwrap_or_default();
    // Use a placeholder string that doesn't look like a Rust format specifier.
    let system = STEERING_SYSTEM_PROMPT.replace("<GRAPH_JSON>", &graph_json);
    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: instruction.to_string(),
        },
    ];
    let diff: GraphDiff = get_structured(llm, messages, graph_diff_schema(), 2).await?;
    Ok(diff)
}
