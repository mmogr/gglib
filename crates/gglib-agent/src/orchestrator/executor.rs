//! Orchestrator execution engine: drive a [`TaskGraph`] to completion.
//!
//! [`execute`] is the single entry point.  It:
//!
//! 1. Calls [`super::director::plan`] to produce a validated [`TaskGraph`].
//! 2. Emits [`OrchestratorEvent::PlanProposed`] and
//!    [`OrchestratorEvent::PlanApproved`] (auto-approved; HITL is Phase D).
//! 3. Runs a topological wave loop: each tick identifies all nodes whose
//!    predecessors are complete and spawns them concurrently, capped by a
//!    `max_worker_concurrency` semaphore.
//! 4. Bridges each worker's [`AgentEvent`] stream to the orchestrator channel
//!    as [`OrchestratorEvent::NodeStarted`] / [`NodeTextDelta`] /
//!    [`NodeToolCallStart`] / [`NodeToolCallComplete`] / [`NodeComplete`] etc.
//! 5. After each worker, runs a compaction pass (hard error — see
//!    [`super::compaction`]).
//! 6. Fails fast on the first worker error: emits [`NodeFailed`] for the
//!    failed node, [`NodeFailed`] for every skipped downstream node, and then
//!    [`OrchestratorError`].
//! 7. After all nodes succeed, delegates to [`super::synthesis::run_synthesis`]
//!    for the final answer.
//!
//! # Context isolation
//!
//! Every worker receives a **fresh** message list:
//! ```text
//! System: "{node.goal}\n\n{predecessor_context_block}"
//! User:   "{node.goal}"
//! ```
//! The director's planning history is **never** injected.  Predecessor context
//! is each predecessor's `compacted_output`, clearly labelled.
//!
//! # Concurrency model
//!
//! Worker tasks run as `tokio::spawn` futures, meaning LLM/tool I/O can
//! overlap across nodes at the same DAG depth.  The semaphore controls how
//! many workers hold the LLM simultaneously; tool I/O stages can always
//! overlap regardless of the cap.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

use gglib_core::domain::orchestrator::events::{ApprovalKind, OrchestratorEvent};
use gglib_core::domain::orchestrator::run::{
    OrchestratorRun, OrchestratorRunEvent, OrchestratorRunStatus,
};
use gglib_core::domain::orchestrator::task_graph::{HitlMode, NodeId, TaskGraph};
use gglib_core::ports::{
    AgentLoopPort, ApprovalDecision, LlmCompletionPort, OrchestratorApprovalRegistryPort,
    OrchestratorRepositoryPort, ToolExecutorPort,
};
use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, ToolDefinition,
};

use crate::AgentLoop;

use super::compaction::{CompactionError, compact_worker_output};
use super::director::{PlanError, plan};
use super::synthesis::run_synthesis;

// =============================================================================
// OrchestratorConfig
// =============================================================================

/// Tuning parameters for a single orchestrator execution run.
pub struct OrchestratorConfig {
    /// Maximum number of director replan attempts after the first.
    ///
    /// Passed directly to [`super::director::plan`].
    pub max_replans: u32,

    /// Maximum number of worker nodes running concurrently.
    ///
    /// Because real LLM generation serialises on a single llama.cpp process,
    /// this cap mainly lets tool I/O stages overlap.  Default: `3`.
    pub max_worker_concurrency: usize,

    /// Human-in-the-loop gate policy.
    pub hitl_mode: HitlMode,

    // ── Phase D additions ─────────────────────────────────────────────────
    /// Process-local approval registry for parking HITL gates.
    ///
    /// If `None`, all gates are auto-approved (backward-compatible).
    pub approval_registry: Option<Arc<dyn OrchestratorApprovalRegistryPort>>,

    /// Repository for persisting run records and events.
    ///
    /// If `None`, persistence is skipped (backward-compatible).
    pub repository: Option<Arc<dyn OrchestratorRepositoryPort>>,

    /// Explicit run id.  When set, no new run record is created; the caller
    /// is responsible for having created the run in the repository already.
    ///
    /// Used by the resume path.
    pub run_id: Option<String>,

    /// Pre-existing graph to use instead of calling the director.
    ///
    /// When set, planning is skipped and execution resumes from the remaining
    /// `Pending` nodes.
    pub graph_override: Option<TaskGraph>,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_replans: 2,
            max_worker_concurrency: 3,
            hitl_mode: HitlMode::None,
            approval_registry: None,
            repository: None,
            run_id: None,
            graph_override: None,
        }
    }
}

// =============================================================================
// ExecuteError
// =============================================================================

/// Error returned by [`execute`].
#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    /// The director failed to produce a valid plan.
    #[error("planning failed: {0}")]
    Plan(#[from] PlanError),

    /// A worker node's output could not be compacted.
    #[error("compaction failed for node '{node_id}': {reason}")]
    CompactionFailed {
        /// Node whose compaction failed.
        node_id: String,
        /// Description of the compaction error.
        reason: String,
    },

    /// A worker node returned an error during execution.
    #[error("worker '{node_id}' failed: {reason}")]
    WorkerFailed {
        /// The failing node id.
        node_id: String,
        /// Error description.
        reason: String,
    },

    /// The plan was rejected by a human reviewer.
    #[error("plan rejected: {reason}")]
    PlanRejected {
        /// Optional user-provided reason.
        reason: String,
    },

    /// A specific node was rejected by a human reviewer.
    #[error("node '{node_id}' rejected: {reason}")]
    NodeRejected {
        /// The rejected node.
        node_id: String,
        /// Optional user-provided reason.
        reason: String,
    },
}

impl From<CompactionError> for ExecuteError {
    fn from(e: CompactionError) -> Self {
        let node_id = match &e {
            CompactionError::EmptyOutput { node_id } | CompactionError::TaskPanic { node_id } => {
                node_id.clone()
            }
            CompactionError::AgentLoop(_) => "unknown".into(),
        };
        Self::CompactionFailed {
            node_id,
            reason: e.to_string(),
        }
    }
}

// =============================================================================
// execute
// =============================================================================

/// Run the full orchestrator pipeline end-to-end.
///
/// Events are sent on `tx`; the caller is responsible for consuming them
/// (either streaming to SSE or rendering to a terminal).  The function drives
/// planning, execution, compaction, and synthesis, then returns.  On success
/// the final event is [`OrchestratorEvent::OrchestratorComplete`].  On
/// failure an [`OrchestratorEvent::OrchestratorError`] is sent before the
/// function returns `Err`.
///
/// # Arguments
///
/// * `goal` — High-level user goal.
/// * `tools` — Tool catalog made available to workers (filtered per-node by
///   each node's `tool_allowlist`).
/// * `llm` — LLM completion port shared across director and all workers.
/// * `tool_executor` — Tool executor shared across all workers.
/// * `config` — Execution tuning parameters.
/// * `tx` — Channel to send orchestrator events on.
#[allow(clippy::too_many_lines, clippy::let_and_return)]
pub async fn execute(
    goal: &str,
    tools: &[ToolDefinition],
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    config: OrchestratorConfig,
    tx: mpsc::Sender<OrchestratorEvent>,
) -> Result<(), ExecuteError> {
    // ── Run ID + persistence bootstrap ───────────────────────────────────────
    let run_id = config
        .run_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let now = chrono::Utc::now().to_rfc3339();

    if config.repository.is_some() && config.run_id.is_none() {
        // Only create a new record if the caller didn't supply an existing run_id.
        let run = OrchestratorRun {
            id: run_id.clone(),
            goal: goal.to_string(),
            graph_json: None,
            status: OrchestratorRunStatus::Running,
            hitl_mode: config.hitl_mode.clone(),
            conversation_id: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        if let Some(repo) = &config.repository {
            if let Err(e) = repo.create_run(run).await {
                tracing::warn!("orchestrator: failed to create run record: {e}");
            }
        }
    }

    // Helper to persist a run event (best-effort — never aborts execution).
    let mut event_seq: i64 = 0;
    let persist_event = |repo: &Option<Arc<dyn OrchestratorRepositoryPort>>,
                         run_id: &str,
                         seq: &mut i64,
                         event: &OrchestratorEvent| {
        let event_json = serde_json::to_string(event).unwrap_or_default();
        let ev = OrchestratorRunEvent {
            run_id: run_id.to_string(),
            seq: *seq,
            event_json,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        *seq += 1;
        (repo.clone(), ev)
    };

    // ── 1. Planning (or graph override for resume) ────────────────────────────
    let graph = if let Some(override_graph) = config.graph_override.clone() {
        // Resume path: skip the director entirely.
        let _ = tx
            .send(OrchestratorEvent::PlanProposed {
                graph: override_graph.clone(),
            })
            .await;
        let _ = tx.send(OrchestratorEvent::PlanApproved).await;
        override_graph
    } else {
        let g = plan(
            goal,
            tools,
            Arc::clone(&llm),
            config.hitl_mode.clone(),
            config.max_replans,
            Some(tx.clone()),
        )
        .await
        .map_err(ExecuteError::Plan)?;

        let _ = tx
            .send(OrchestratorEvent::PlanProposed { graph: g.clone() })
            .await;

        // ── 1a. Plan HITL gate ────────────────────────────────────────────────
        let approved_graph = if config.hitl_mode >= HitlMode::ApprovePlan {
            if let Some(registry) = &config.approval_registry {
                let approval_id = uuid::Uuid::new_v4().to_string();
                let (tx_approval, rx_approval) =
                    tokio::sync::oneshot::channel::<ApprovalDecision>();
                registry.register(approval_id.clone(), tx_approval);

                let event = OrchestratorEvent::AwaitingApproval {
                    approval_id: approval_id.clone(),
                    kind: ApprovalKind::Plan,
                };
                let _ = tx.send(event.clone()).await;
                let (repo_clone, ev) =
                    persist_event(&config.repository, &run_id, &mut event_seq, &event);
                if let Some(repo) = &repo_clone {
                    if let Err(e) = repo.append_event(ev).await {
                        tracing::warn!("orchestrator: failed to persist event: {e}");
                    }
                    if let Err(e) = repo
                        .update_run_status(&run_id, OrchestratorRunStatus::AwaitingApproval)
                        .await
                    {
                        tracing::warn!("orchestrator: failed to update run status: {e}");
                    }
                }

                match rx_approval.await {
                    Ok(ApprovalDecision::Approve) => {
                        let _ = tx.send(OrchestratorEvent::PlanApproved).await;
                        if let Some(repo) = &config.repository {
                            let _ = repo
                                .update_run_status(&run_id, OrchestratorRunStatus::Running)
                                .await;
                        }
                        g.clone()
                    }
                    Ok(ApprovalDecision::ApproveWithEdits(edited)) => {
                        let _ = tx.send(OrchestratorEvent::PlanApproved).await;
                        if let Some(repo) = &config.repository {
                            if let Ok(json) = serde_json::to_string(&*edited) {
                                let _ = repo.update_graph(&run_id, &json).await;
                            }
                            let _ = repo
                                .update_run_status(&run_id, OrchestratorRunStatus::Running)
                                .await;
                        }
                        *edited
                    }
                    Ok(ApprovalDecision::Reject(reason)) => {
                        let reject_event = OrchestratorEvent::PlanRejected {
                            reason: Some(reason.clone()),
                        };
                        let _ = tx.send(reject_event).await;
                        if let Some(repo) = &config.repository {
                            let _ = repo
                                .update_run_status(&run_id, OrchestratorRunStatus::Failed)
                                .await;
                        }
                        let _ = tx
                            .send(OrchestratorEvent::OrchestratorError {
                                message: format!("plan rejected: {reason}"),
                            })
                            .await;
                        return Err(ExecuteError::PlanRejected { reason });
                    }
                    Err(_) => {
                        // Registry dropped — treat as rejection (process restart).
                        if let Some(repo) = &config.repository {
                            let _ = repo
                                .update_run_status(&run_id, OrchestratorRunStatus::Interrupted)
                                .await;
                        }
                        return Err(ExecuteError::PlanRejected {
                            reason: "approval channel closed".into(),
                        });
                    }
                }
            } else {
                // No registry — auto-approve.
                let _ = tx.send(OrchestratorEvent::PlanApproved).await;
                g.clone()
            }
        } else {
            let _ = tx.send(OrchestratorEvent::PlanApproved).await;
            g.clone()
        };

        approved_graph
    };

    // Persist the graph after plan approval.
    if let Some(repo) = &config.repository {
        if let Ok(json) = serde_json::to_string(&graph) {
            if let Err(e) = repo.update_graph(&run_id, &json).await {
                tracing::warn!("orchestrator: failed to persist graph: {e}");
            }
        }
    }

    // ── 2. Topological wave execution ────────────────────────────────────────
    // For resume: pre-populate `completed` with Done nodes.
    let pre_completed: HashSet<NodeId> = graph
        .nodes
        .iter()
        .filter(|(_, n)| n.status == gglib_core::domain::orchestrator::task_graph::NodeStatus::Done)
        .map(|(id, _)| id.clone())
        .collect();

    let result = run_wave_loop(
        goal,
        &graph,
        pre_completed,
        Arc::clone(&llm),
        Arc::clone(&tool_executor),
        &config,
        &run_id,
        &mut event_seq,
        &tx,
    )
    .await;

    match result {
        Ok(compacted) => {
            if let Some(repo) = &config.repository {
                if let Ok(json) = serde_json::to_string(&graph) {
                    let _ = repo.update_graph(&run_id, &json).await;
                }
                let _ = repo
                    .update_run_status(&run_id, OrchestratorRunStatus::Completed)
                    .await;
            }
            // ── 3. Synthesis ─────────────────────────────────────────────────
            run_synthesis(&graph, &compacted, &llm, &tool_executor, &tx).await;
            Ok(())
        }
        Err(e) => {
            if let Some(repo) = &config.repository {
                let _ = repo
                    .update_run_status(&run_id, OrchestratorRunStatus::Failed)
                    .await;
            }
            let _ = tx
                .send(OrchestratorEvent::OrchestratorError {
                    message: e.to_string(),
                })
                .await;
            Err(e)
        }
    }
}

// =============================================================================
// Topological wave loop
// =============================================================================

/// Drive the graph to completion wave by wave.
///
/// Returns a map of `NodeId → compacted_output` for every successfully
/// completed node, or an [`ExecuteError`] on the first failure.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn run_wave_loop(
    _goal: &str,
    graph: &TaskGraph,
    mut completed: HashSet<NodeId>,
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    config: &OrchestratorConfig,
    run_id: &str,
    event_seq: &mut i64,
    tx: &mpsc::Sender<OrchestratorEvent>,
) -> Result<HashMap<NodeId, String>, ExecuteError> {
    let sem = Arc::new(Semaphore::new(config.max_worker_concurrency));
    let mut compacted: HashMap<NodeId, String> = HashMap::new();

    loop {
        let ready = graph.ready_nodes(&completed);
        if ready.is_empty() {
            break;
        }

        // ── Node HITL gates (serial, before spawning the wave) ────────────────
        if config.hitl_mode >= HitlMode::ApproveEachNode {
            if let Some(registry) = &config.approval_registry {
                for node_id in &ready {
                    let approval_id = uuid::Uuid::new_v4().to_string();
                    let (tx_approval, rx_approval) =
                        tokio::sync::oneshot::channel::<ApprovalDecision>();
                    registry.register(approval_id.clone(), tx_approval);

                    let node_goal = graph.nodes[node_id].goal.clone();
                    let event = OrchestratorEvent::AwaitingApproval {
                        approval_id: approval_id.clone(),
                        kind: ApprovalKind::Node {
                            node_id: node_id.0.clone(),
                        },
                    };
                    let _ = tx.send(event.clone()).await;
                    if let Some(repo) = &config.repository {
                        let ev = OrchestratorRunEvent {
                            run_id: run_id.to_string(),
                            seq: *event_seq,
                            event_json: serde_json::to_string(&event).unwrap_or_default(),
                            created_at: chrono::Utc::now().to_rfc3339(),
                        };
                        *event_seq += 1;
                        if let Err(e) = repo.append_event(ev).await {
                            tracing::warn!("orchestrator: persist event error: {e}");
                        }
                        if let Err(e) = repo
                            .update_run_status(run_id, OrchestratorRunStatus::AwaitingApproval)
                            .await
                        {
                            tracing::warn!("orchestrator: status update error: {e}");
                        }
                    }

                    match rx_approval.await {
                        Ok(ApprovalDecision::Approve | ApprovalDecision::ApproveWithEdits(_)) => {
                            if let Some(repo) = &config.repository {
                                let _ = repo
                                    .update_run_status(run_id, OrchestratorRunStatus::Running)
                                    .await;
                            }
                        }
                        Ok(ApprovalDecision::Reject(reason)) => {
                            let id_str = node_id.0.clone();
                            let _ = tx
                                .send(OrchestratorEvent::NodeFailed {
                                    node_id: id_str.clone(),
                                    error: format!("rejected: {reason}"),
                                })
                                .await;
                            return Err(ExecuteError::NodeRejected {
                                node_id: id_str,
                                reason,
                            });
                        }
                        Err(_) => {
                            return Err(ExecuteError::NodeRejected {
                                node_id: node_id.0.clone(),
                                reason: "approval channel closed".into(),
                            });
                        }
                    }
                    drop(node_goal); // used only for future context attach
                }
            }
        }

        // Spawn all ready nodes concurrently, bounded by the semaphore.
        #[allow(clippy::type_complexity)]
        let mut join_set: JoinSet<(NodeId, Result<(String, String), ExecuteError>)> =
            JoinSet::new();

        for node_id in ready {
            let node = &graph.nodes[node_id];
            let node_id_clone = node_id.clone();
            let node_goal = node.goal.clone();
            let allowlist: Option<HashSet<String>> = if node.tool_allowlist.is_empty() {
                Some(HashSet::new())
            } else {
                Some(node.tool_allowlist.iter().cloned().collect())
            };

            // Build predecessor context from already-compacted outputs.
            let predecessor_context = build_predecessor_context(node_id, graph, &compacted);

            let sem_clone = Arc::clone(&sem);
            let llm_clone = Arc::clone(&llm);
            let tool_executor_clone = Arc::clone(&tool_executor);
            let tx_clone = tx.clone();

            join_set.spawn(async move {
                let _permit = sem_clone
                    .acquire_owned()
                    .await
                    .expect("semaphore not closed");

                let result = run_worker(
                    &node_id_clone,
                    &node_goal,
                    predecessor_context,
                    allowlist,
                    llm_clone,
                    tool_executor_clone,
                    &tx_clone,
                )
                .await;

                (node_id_clone, result)
            });
        }

        // Collect results; fail fast on first error.
        while let Some(join_result) = join_set.join_next().await {
            let (node_id, worker_result) = join_result.expect("join_set task panicked");

            match worker_result {
                Ok((_full_output, compacted_output)) => {
                    compacted.insert(node_id.clone(), compacted_output);
                    completed.insert(node_id);
                }
                Err(e) => {
                    // Abort the remaining in-flight tasks.
                    join_set.abort_all();
                    // Drain the join set so all tasks are cancelled.
                    while join_set.join_next().await.is_some() {}
                    return Err(e);
                }
            }
        }
    }

    Ok(compacted)
}

// =============================================================================
// Single worker execution
// =============================================================================

/// Run a single worker node as an isolated [`AgentLoop`] and return
/// `(full_output, compacted_output)` on success.
///
/// Emits [`NodeStarted`], then bridges the agent's event stream, and finishes
/// with [`NodeCompacting`] → [`NodeComplete`] or [`NodeFailed`].
#[allow(clippy::too_many_lines)]
async fn run_worker(
    node_id: &NodeId,
    node_goal: &str,
    predecessor_context: String,
    tool_filter: Option<HashSet<String>>,
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    tx: &mpsc::Sender<OrchestratorEvent>,
) -> Result<(String, String), ExecuteError> {
    let id_str = node_id.0.clone();

    let _ = tx
        .send(OrchestratorEvent::NodeStarted {
            node_id: id_str.clone(),
            goal: node_goal.to_owned(),
        })
        .await;

    // Build the worker's isolated message list.
    // Director planning history is never included here.
    let system_content = if predecessor_context.is_empty() {
        node_goal.to_owned()
    } else {
        format!("{node_goal}\n\n{predecessor_context}")
    };

    let messages = vec![
        AgentMessage::System {
            content: system_content,
        },
        AgentMessage::User {
            content: node_goal.to_owned(),
        },
    ];

    let agent: Arc<dyn AgentLoopPort> =
        AgentLoop::build(Arc::clone(&llm), Arc::clone(&tool_executor), tool_filter);

    let (agent_tx, mut agent_rx) =
        tokio::sync::mpsc::channel::<AgentEvent>(AGENT_EVENT_CHANNEL_CAPACITY);

    let config = AgentConfig::default();

    let handle = {
        let agent = Arc::clone(&agent);
        tokio::spawn(async move { agent.run(messages, config, agent_tx).await })
    };

    // Bridge agent events to orchestrator node events.
    let mut final_answer: Option<String> = None;
    let mut worker_error: Option<String> = None;

    while let Some(event) = agent_rx.recv().await {
        match event {
            AgentEvent::TextDelta { content: delta } => {
                let _ = tx
                    .send(OrchestratorEvent::NodeTextDelta {
                        node_id: id_str.clone(),
                        delta,
                    })
                    .await;
            }
            AgentEvent::ReasoningDelta { content: delta } => {
                let _ = tx
                    .send(OrchestratorEvent::NodeReasoningDelta {
                        node_id: id_str.clone(),
                        delta,
                    })
                    .await;
            }
            AgentEvent::PromptProgress {
                processed,
                total,
                cached,
                time_ms,
            } => {
                let _ = tx
                    .send(OrchestratorEvent::NodeProgress {
                        node_id: id_str.clone(),
                        processed,
                        total,
                        cached,
                        time_ms,
                    })
                    .await;
            }
            AgentEvent::ToolCallStart {
                tool_call,
                display_name,
                args_summary,
            } => {
                let _ = tx
                    .send(OrchestratorEvent::NodeToolCallStart {
                        node_id: id_str.clone(),
                        tool_call,
                        display_name,
                        args_summary,
                    })
                    .await;
            }
            AgentEvent::ToolCallComplete {
                tool_name,
                result,
                display_name,
                duration_display,
                ..
            } => {
                let _ = tx
                    .send(OrchestratorEvent::NodeToolCallComplete {
                        node_id: id_str.clone(),
                        tool_name,
                        result,
                        display_name,
                        duration_display,
                    })
                    .await;
            }
            AgentEvent::SystemWarning {
                message,
                suggested_action,
            } => {
                let _ = tx
                    .send(OrchestratorEvent::NodeSystemWarning {
                        node_id: id_str.clone(),
                        message,
                        suggested_action,
                    })
                    .await;
            }
            AgentEvent::FinalAnswer { content } => {
                final_answer = Some(content);
            }
            AgentEvent::Error { message } => {
                worker_error = Some(message);
            }
            AgentEvent::IterationComplete { .. } => {}
        }
    }

    // Propagate join errors (panics).
    match handle.await {
        Err(_) => {
            let msg = format!("worker task panicked for node '{id_str}'");
            let _ = tx
                .send(OrchestratorEvent::NodeFailed {
                    node_id: id_str.clone(),
                    error: msg.clone(),
                })
                .await;
            return Err(ExecuteError::WorkerFailed {
                node_id: id_str,
                reason: msg,
            });
        }
        Ok(Err(agent_err)) => {
            let msg = agent_err.to_string();
            let _ = tx
                .send(OrchestratorEvent::NodeFailed {
                    node_id: id_str.clone(),
                    error: msg.clone(),
                })
                .await;
            return Err(ExecuteError::WorkerFailed {
                node_id: id_str,
                reason: msg,
            });
        }
        Ok(Ok(_)) => {}
    }

    if let Some(err_msg) = worker_error {
        let _ = tx
            .send(OrchestratorEvent::NodeFailed {
                node_id: id_str.clone(),
                error: err_msg.clone(),
            })
            .await;
        return Err(ExecuteError::WorkerFailed {
            node_id: id_str,
            reason: err_msg,
        });
    }

    let full_output = final_answer.unwrap_or_default();

    // ── Compaction (hard error) ───────────────────────────────────────────────
    let _ = tx
        .send(OrchestratorEvent::NodeCompacting {
            node_id: id_str.clone(),
        })
        .await;

    let compacted =
        match compact_worker_output(&id_str, node_goal, &full_output, &llm, &tool_executor).await {
            Ok(s) => s,
            Err(e) => {
                let msg = e.to_string();
                let _ = tx
                    .send(OrchestratorEvent::NodeFailed {
                        node_id: id_str.clone(),
                        error: msg.clone(),
                    })
                    .await;
                return Err(ExecuteError::CompactionFailed {
                    node_id: id_str,
                    reason: msg,
                });
            }
        };

    let preview: String = full_output.chars().take(200).collect();
    let _ = tx
        .send(OrchestratorEvent::NodeComplete {
            node_id: id_str,
            output_preview: preview,
        })
        .await;

    Ok((full_output, compacted))
}

// =============================================================================
// Context helpers
// =============================================================================

/// Build the predecessor context block injected into a worker's system prompt.
///
/// Only nodes listed in `node_id`'s `depends_on` are included — not the
/// full graph history.  This keeps each worker's context window minimal.
fn build_predecessor_context(
    node_id: &NodeId,
    graph: &TaskGraph,
    compacted: &HashMap<NodeId, String>,
) -> String {
    let node = &graph.nodes[node_id];
    if node.depends_on.is_empty() {
        return String::new();
    }

    let mut parts: Vec<String> = Vec::new();
    for dep_id in &node.depends_on {
        if let Some(text) = compacted.get(dep_id) {
            parts.push(format!(
                "Context from predecessor '{}':\n{}",
                dep_id.0, text
            ));
        }
    }

    if parts.is_empty() {
        return String::new();
    }

    parts.join("\n\n")
}

// =============================================================================
// Unit tests (no LLM required)
// =============================================================================

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    #[allow(unused_imports)]
    use super::build_predecessor_context;
    use gglib_core::domain::orchestrator::task_graph::{
        HitlMode, NodeId, NodeStatus, TaskGraph, TaskNode,
    };

    fn make_node(id: &str, deps: &[&str]) -> TaskNode {
        TaskNode {
            id: NodeId(id.into()),
            goal: id.into(),
            depends_on: deps.iter().map(|d| NodeId((*d).to_string())).collect(),
            tool_allowlist: vec![],
            status: NodeStatus::Pending,
            output: None,
            compacted_output: None,
            error: None,
        }
    }

    #[test]
    fn no_predecessors_returns_empty() {
        let graph =
            TaskGraph::new("g".into(), HitlMode::None, vec![make_node("root", &[])]).unwrap();
        let compacted = HashMap::new();
        let ctx = build_predecessor_context(&NodeId("root".into()), &graph, &compacted);
        assert!(ctx.is_empty());
    }

    #[test]
    fn predecessor_context_is_injected() {
        let graph = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![make_node("a", &[]), make_node("b", &["a"])],
        )
        .unwrap();
        let mut compacted = HashMap::new();
        compacted.insert(NodeId("a".into()), "Result of A".into());
        let ctx = build_predecessor_context(&NodeId("b".into()), &graph, &compacted);
        assert!(ctx.contains("Context from predecessor 'a'"));
        assert!(ctx.contains("Result of A"));
    }

    #[test]
    fn planning_history_not_in_context() {
        // Verify that context assembly only looks at depends_on, not the
        // full compacted map (director history isolation).
        let graph = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![make_node("x", &[]), make_node("y", &[])],
        )
        .unwrap();
        let mut compacted = HashMap::new();
        compacted.insert(NodeId("x".into()), "X result".into());
        // y does not depend on x, so x's output must NOT appear in y's context.
        let ctx = build_predecessor_context(&NodeId("y".into()), &graph, &compacted);
        assert!(ctx.is_empty(), "y must not see x's output: got {ctx:?}");
    }
}
