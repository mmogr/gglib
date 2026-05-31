//! Post-worker compaction for the orchestrator executor.
//!
//! Unlike the council's compaction (which degrades gracefully when parsing
//! fails), orchestrator compaction is a **hard error**: a failure here aborts
//! the run immediately because downstream workers rely on the compacted output
//! as context.
//!
//! # Why hard-error?
//!
//! Worker outputs can be large (multi-turn tool-using sessions). If compaction
//! silently falls back to passing the raw output, context windows for
//! downstream nodes grow unpredictably and may exceed the model's limit.
//! Failing loudly keeps the system predictable and forces operators to
//! investigate root causes.

use std::collections::HashSet;
use std::sync::Arc;

use gglib_core::ports::{AgentError, LlmCompletionPort, ToolExecutorPort};
use gglib_core::{AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage};

use crate::AgentLoop;

use super::prompts::WORKER_COMPACTION_PROMPT;

// =============================================================================
// CompactionError
// =============================================================================

/// Error returned when worker compaction fails.
///
/// Both variants indicate a hard failure — the executor must emit
/// [`gglib_core::domain::council::events::CouncilEvent::NodeFailed`]
/// and abort remaining work.
#[derive(Debug, thiserror::Error)]
pub enum CompactionError {
    /// The compaction agent loop returned an error.
    #[error("agent loop error during compaction: {0}")]
    AgentLoop(#[from] AgentError),

    /// The agent loop completed but produced empty output.
    #[error("compaction produced empty output for node '{node_id}'")]
    EmptyOutput {
        /// The node whose output could not be compacted.
        node_id: String,
    },

    /// The tokio task hosting the agent loop panicked.
    #[error("compaction task panicked for node '{node_id}'")]
    TaskPanic {
        /// The node whose compaction task panicked.
        node_id: String,
    },
}

// =============================================================================
// Public API
// =============================================================================

/// Run a single-iteration compaction agent on `output` and return the
/// condensed summary.
///
/// Sends the full worker output to a no-tool `AgentLoop` instructed to
/// produce a ≤ 250-word summary.  Returns [`CompactionError`] on any failure
/// (agent error, empty output, task panic).
///
/// # Arguments
///
/// * `node_id` — used only in error messages.
/// * `node_goal` — goal that the worker was given; injected into the prompt.
/// * `output` — the worker's full output text to compact.
/// * `llm` — the LLM completion port to use.
/// * `tool_executor` — used to construct an `AgentLoop` (no tools granted).
pub(super) async fn compact_worker_output(
    node_id: &str,
    node_goal: &str,
    output: &str,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
) -> Result<String, CompactionError> {
    #[allow(clippy::literal_string_with_formatting_args)]
    let system = WORKER_COMPACTION_PROMPT
        .replace("{goal}", node_goal)
        .replace("{output}", output);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: "Summarise the worker output.".into(),
        },
    ];

    // Compaction uses no tools — pure text generation, single iteration.
    let agent = AgentLoop::build(
        Arc::clone(llm),
        Arc::clone(tool_executor),
        Some(HashSet::new()),
    );
    let mut config = AgentConfig::default();
    config.max_iterations = 1;

    let (agent_tx, mut agent_rx) =
        tokio::sync::mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let handle = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move { agent.run(messages, config, agent_tx).await })
    };

    // Drain the event channel; only FinalAnswer matters here.
    let mut compacted: Option<String> = None;
    while let Some(event) = agent_rx.recv().await {
        if let AgentEvent::FinalAnswer { content } = event {
            compacted = Some(content);
        }
    }

    // Propagate join errors (panics).
    match handle.await {
        Err(_) => {
            return Err(CompactionError::TaskPanic {
                node_id: node_id.to_owned(),
            });
        }
        Ok(Err(e)) => return Err(CompactionError::AgentLoop(e)),
        Ok(Ok(_)) => {}
    }

    let summary = compacted.unwrap_or_default();
    if summary.trim().is_empty() {
        return Err(CompactionError::EmptyOutput {
            node_id: node_id.to_owned(),
        });
    }

    Ok(summary)
}
