//! Synthesis pass for the orchestrator executor.
//!
//! After all DAG nodes complete, the synthesiser assembles the compacted
//! outputs from every leaf node into a single unified answer that addresses
//! the original goal.  Text is streamed as
//! [`OrchestratorEvent::SynthesisTextDelta`] events.
//!
//! A *leaf node* is any node that no other node declares as a dependency —
//! i.e. nodes with no successors in the DAG.

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::mpsc;

use gglib_core::ports::{LlmCompletionPort, ToolExecutorPort};
use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, NodeId, OrchestratorEvent,
    TaskGraph,
};

use crate::AgentLoop;

use super::prompts::ORCHESTRATOR_SYNTHESIS_PROMPT;

// =============================================================================
// Public API
// =============================================================================

/// Run the synthesis pass and stream results onto `tx`.
///
/// Emits (in order):
/// 1. [`OrchestratorEvent::SynthesisStart`]
/// 2. Zero or more [`OrchestratorEvent::SynthesisTextDelta`] events.
/// 3. [`OrchestratorEvent::SynthesisComplete`] with the full answer.
/// 4. [`OrchestratorEvent::OrchestratorComplete`] with the same answer.
///
/// Any agent-loop failure is propagated to `tx` as
/// [`OrchestratorEvent::OrchestratorError`]; the function then returns
/// normally so the caller can close the channel cleanly.
///
/// # Arguments
///
/// * `graph` — The completed task graph (used to identify leaf nodes).
/// * `compacted` — Map of node id → compacted output, one entry per node.
/// * `llm` — LLM completion port.
/// * `tool_executor` — Used to build the synthesis `AgentLoop` (no tools).
/// * `tx` — Orchestrator event sender.
#[allow(clippy::too_many_lines)]
pub(super) async fn run_synthesis(
    graph: &TaskGraph,
    compacted: &std::collections::HashMap<NodeId, String>,
    llm: &Arc<dyn LlmCompletionPort>,
    tool_executor: &Arc<dyn ToolExecutorPort>,
    tx: &mpsc::Sender<OrchestratorEvent>,
) {
    // Identify leaf nodes: those that no other node lists as a dependency.
    let all_deps: HashSet<&NodeId> = graph
        .nodes
        .values()
        .flat_map(|n| n.depends_on.iter())
        .collect();

    // Collect leaf compacted outputs; fall back to all nodes if no leaves found.
    let mut leaf_results: Vec<(&NodeId, &str)> = graph
        .nodes
        .keys()
        .filter(|id| !all_deps.contains(*id))
        .filter_map(|id| compacted.get(id).map(|s| (id, s.as_str())))
        .collect();

    // Stable ordering for deterministic prompts.
    leaf_results.sort_by_key(|(id, _)| &id.0);

    if leaf_results.is_empty() {
        // Fallback: use all compacted outputs if the leaf set is empty (e.g.
        // single-node graph where node has deps back to itself, which should be
        // impossible after validation, but defensive).
        let mut all: Vec<(&NodeId, &str)> =
            compacted.iter().map(|(id, s)| (id, s.as_str())).collect();
        all.sort_by_key(|(id, _)| &id.0);
        leaf_results = all;
    }

    // Build the results block.
    let results_block = leaf_results
        .iter()
        .map(|(id, content)| format!("[{}]:\n{}", id.0, content))
        .collect::<Vec<_>>()
        .join("\n\n");

    #[allow(clippy::literal_string_with_formatting_args)]
    let system = ORCHESTRATOR_SYNTHESIS_PROMPT
        .replace("{goal}", &graph.goal)
        .replace("{results}", &results_block);

    let messages = vec![
        AgentMessage::System { content: system },
        AgentMessage::User {
            content: graph.goal.clone(),
        },
    ];

    // Synthesiser has no tools — single-iteration pure generation.
    let agent = AgentLoop::build(
        Arc::clone(llm),
        Arc::clone(tool_executor),
        Some(HashSet::new()),
    );
    let mut config = AgentConfig::default();
    config.max_iterations = 1;

    let (agent_tx, mut agent_rx) =
        tokio::sync::mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let _ = tx.send(OrchestratorEvent::SynthesisStart).await;

    let handle = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move { agent.run(messages, config, agent_tx).await })
    };

    // Bridge agent events → orchestrator synthesis events.
    let mut answer: Option<String> = None;
    let mut has_streamed = false;

    while let Some(event) = agent_rx.recv().await {
        match event {
            AgentEvent::TextDelta { content: delta } => {
                has_streamed = true;
                let _ = tx
                    .send(OrchestratorEvent::SynthesisTextDelta { delta })
                    .await;
            }
            AgentEvent::PromptProgress {
                processed,
                total,
                cached,
                time_ms,
            } => {
                let _ = tx
                    .send(OrchestratorEvent::SynthesisProgress {
                        processed,
                        total,
                        cached,
                        time_ms,
                    })
                    .await;
            }
            AgentEvent::FinalAnswer { content } => {
                answer = Some(content);
            }
            AgentEvent::Error { message } => {
                let _ = tx
                    .send(OrchestratorEvent::OrchestratorError {
                        message: format!("Synthesis failed: {message}"),
                    })
                    .await;
                // Drain the handle and return; caller sees OrchestratorError.
                let _ = handle.await;
                return;
            }
            _ => {}
        }
    }

    match handle.await {
        Err(_) | Ok(Err(_)) => {
            let _ = tx
                .send(OrchestratorEvent::OrchestratorError {
                    message: "Synthesis task panicked".into(),
                })
                .await;
            return;
        }
        Ok(Ok(_)) => {}
    }

    let content = answer.unwrap_or_default();

    // Safety net: if FinalAnswer arrived but no TextDelta was streamed, emit
    // the full content now so the frontend sees it.
    if !has_streamed && !content.is_empty() {
        let _ = tx
            .send(OrchestratorEvent::SynthesisTextDelta {
                delta: content.clone(),
            })
            .await;
    }

    let _ = tx
        .send(OrchestratorEvent::SynthesisComplete {
            content: content.clone(),
        })
        .await;
    let _ = tx
        .send(OrchestratorEvent::OrchestratorComplete { answer: content })
        .await;
}
