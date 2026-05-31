//! Orchestrator execution engine: drive a [`TaskGraph`] to completion.
//!
//! [`execute`] is the single entry point.  It:
//!
//! 1. Calls [`super::director::plan`] to produce a validated [`TaskGraph`].
//! 2. Emits [`CouncilEvent::PlanProposed`] and
//!    [`CouncilEvent::PlanApproved`] (auto-approved; HITL is Phase D).
//! 3. Runs a topological wave loop: each tick identifies all nodes whose
//!    predecessors are complete and spawns them concurrently, capped by a
//!    `max_worker_concurrency` semaphore.
//! 4. Bridges each worker's [`AgentEvent`] stream to the orchestrator channel
//!    as [`CouncilEvent::NodeStarted`] / [`NodeTextDelta`] /
//!    [`NodeToolCallStart`] / [`NodeToolCallComplete`] / [`NodeComplete`] etc.
//! 5. After each worker, runs a compaction pass (hard error — see
//!    [`super::compaction`]).
//! 6. Fails fast on the first worker error: emits [`NodeFailed`] for the
//!    failed node, [`NodeFailed`] for every skipped downstream node, and then
//!    [`CouncilError`].
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

use gglib_core::domain::council::events::{ApprovalKind, CouncilEvent};
use gglib_core::domain::council::run::{CouncilRun, CouncilRunEvent, CouncilRunStatus};
use gglib_core::domain::council::task_graph::{
    HitlMode, NodeBudget, NodeId, TaskGraph, TaskNodeKind,
};
use gglib_core::ports::{
    AgentLoopPort, ApprovalDecision, CouncilApprovalRegistryPort, CouncilRepositoryPort,
    LlmCompletionPort, ToolExecutorPort,
};
use gglib_core::{
    AGENT_EVENT_CHANNEL_CAPACITY, AgentConfig, AgentEvent, AgentMessage, ToolDefinition,
};

use tokio_util::sync::CancellationToken;

use crate::AgentLoop;

use super::compaction::{CompactionError, compact_worker_output};
use super::debate;
use super::director::PlanError;
use super::estimator::estimate_run_cost;
use super::planner::plan;
use super::spawn::{SpawnCapturingExecutor, SpawnRequest, SpawnSink};
use super::steering::NoteQueue;
use super::synthesis::run_synthesis;

// =============================================================================
// CouncilConfig
// =============================================================================

/// Tuning parameters for a single orchestrator execution run.
#[derive(Clone)]
pub struct CouncilConfig {
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
    pub approval_registry: Option<Arc<dyn CouncilApprovalRegistryPort>>,

    /// Repository for persisting run records and events.
    ///
    /// If `None`, persistence is skipped (backward-compatible).
    pub repository: Option<Arc<dyn CouncilRepositoryPort>>,

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

    /// Maximum recursion depth for nested Team and spawn sub-team graphs.
    ///
    /// A `debug_assert` fires at this limit during development.  The value is
    /// intentionally generous (default: 9 = 3 levels × 3 max teams each) to
    /// catch infinite-recursion bugs without restricting real use cases.
    pub max_team_depth: u32,

    /// Advisory node-count budget used by the Phase J cost estimator.
    ///
    /// When the aggregate node count exceeds `node_budget.upper_bound()`, the
    /// executor emits a [`tracing::warn!`] but **never** returns an error.
    /// Defaults to [`NodeBudget::TaskForce`] (25 nodes).
    pub node_budget: NodeBudget,

    /// Optional per-run steering note queue.
    ///
    /// When set, the executor drains this queue at each wave boundary
    /// (depth 0 only).  For each queued instruction it calls the steering LLM,
    /// applies the returned [`GraphDiff`], and emits a
    /// [`CouncilEvent::SteeringApplied`] event.  Instructions are
    /// silently discarded on parse or validation failure (a warning is logged).
    ///
    /// `None` disables steering (backward-compatible).
    pub note_queue: Option<NoteQueue>,

    /// Phase M: if set, execution resumes **from** this wave index.
    ///
    /// The caller is responsible for having already truncated events after
    /// this wave via [`CouncilRepositoryPort::truncate_events_after_wave`]
    /// and for having reset the graph nodes in those later waves back to
    /// `Pending`.  The executor will start `wave_number` at `rewind_to_wave`
    /// so that newly-emitted events carry correct wave indices.
    ///
    /// `None` disables rewind behaviour (backward-compatible).
    pub rewind_to_wave: Option<u32>,
}

impl Default for CouncilConfig {
    fn default() -> Self {
        Self {
            max_replans: 2,
            max_worker_concurrency: 3,
            hitl_mode: HitlMode::None,
            approval_registry: None,
            repository: None,
            run_id: None,
            graph_override: None,
            max_team_depth: 9,
            node_budget: NodeBudget::default(),
            note_queue: None,
            rewind_to_wave: None,
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

    /// The executor hit the maximum recursive team depth.
    #[error("maximum team recursion depth exceeded")]
    MaxDepthExceeded,
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
/// the final event is [`CouncilEvent::CouncilComplete`].  On
/// failure an [`CouncilEvent::CouncilError`] is sent before the
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
    config: CouncilConfig,
    tx: mpsc::Sender<CouncilEvent>,
) -> Result<(), ExecuteError> {
    // ── Run ID + persistence bootstrap ───────────────────────────────────────
    let run_id = config
        .run_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let now = chrono::Utc::now().to_rfc3339();

    if config.repository.is_some() && config.run_id.is_none() {
        // Only create a new record if the caller didn't supply an existing run_id.
        let run = CouncilRun {
            id: run_id.clone(),
            goal: goal.to_string(),
            graph_json: None,
            status: CouncilRunStatus::Running,
            hitl_mode: config.hitl_mode.clone(),
            conversation_id: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        if let Some(repo) = &config.repository {
            if let Err(e) = repo.create_run(run).await {
                tracing::warn!("council: failed to create run record: {e}");
            }
        }
    }

    // Helper to persist a run event (best-effort — never aborts execution).
    let mut event_seq: i64 = 0;
    let persist_event = |repo: &Option<Arc<dyn CouncilRepositoryPort>>,
                         run_id: &str,
                         seq: &mut i64,
                         event: &CouncilEvent| {
        let event_json = serde_json::to_string(event).unwrap_or_default();
        let ev = CouncilRunEvent {
            run_id: run_id.to_string(),
            seq: *seq,
            event_json,
            created_at: chrono::Utc::now().to_rfc3339(),
            wave_index: 0, // pre-execution events always belong to wave 0
        };
        *seq += 1;
        (repo.clone(), ev)
    };

    // ── 1. Planning (or graph override for resume) ────────────────────────────
    let mut graph = if let Some(override_graph) = config.graph_override.clone() {
        // Resume path: skip the director entirely.
        let _ = tx
            .send(CouncilEvent::PlanProposed {
                graph: override_graph.clone(),
            })
            .await;
        let cost = estimate_run_cost(&override_graph);
        let _ = tx
            .send(CouncilEvent::RunCostEstimate {
                node_count: cost.node_count,
                est_tokens: cost.est_tokens,
                est_wall_seconds: cost.est_wall_seconds,
            })
            .await;
        if override_graph.total_node_count() > config.node_budget.upper_bound() {
            tracing::warn!(
                node_count = override_graph.total_node_count(),
                budget_upper = config.node_budget.upper_bound(),
                "council: aggregate node count exceeds advisory node budget",
            );
        }
        let _ = tx.send(CouncilEvent::PlanApproved).await;
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
            .send(CouncilEvent::PlanProposed { graph: g.clone() })
            .await;
        let cost = estimate_run_cost(&g);
        let _ = tx
            .send(CouncilEvent::RunCostEstimate {
                node_count: cost.node_count,
                est_tokens: cost.est_tokens,
                est_wall_seconds: cost.est_wall_seconds,
            })
            .await;
        if g.total_node_count() > config.node_budget.upper_bound() {
            tracing::warn!(
                node_count = g.total_node_count(),
                budget_upper = config.node_budget.upper_bound(),
                "council: aggregate node count exceeds advisory node budget",
            );
        }

        // ── 1a. Plan HITL gate ────────────────────────────────────────────────
        let approved_graph = if config.hitl_mode >= HitlMode::ApprovePlan {
            if let Some(registry) = &config.approval_registry {
                let approval_id = uuid::Uuid::new_v4().to_string();
                let (tx_approval, rx_approval) =
                    tokio::sync::oneshot::channel::<ApprovalDecision>();
                registry.register(approval_id.clone(), tx_approval);

                let event = CouncilEvent::AwaitingApproval {
                    approval_id: approval_id.clone(),
                    kind: ApprovalKind::Plan,
                };
                let _ = tx.send(event.clone()).await;
                let (repo_clone, ev) =
                    persist_event(&config.repository, &run_id, &mut event_seq, &event);
                if let Some(repo) = &repo_clone {
                    if let Err(e) = repo.append_event(ev).await {
                        tracing::warn!("council: failed to persist event: {e}");
                    }
                    if let Err(e) = repo
                        .update_run_status(&run_id, CouncilRunStatus::AwaitingApproval)
                        .await
                    {
                        tracing::warn!("council: failed to update run status: {e}");
                    }
                }

                match rx_approval.await {
                    Ok(ApprovalDecision::Approve) => {
                        let _ = tx.send(CouncilEvent::PlanApproved).await;
                        if let Some(repo) = &config.repository {
                            let _ = repo
                                .update_run_status(&run_id, CouncilRunStatus::Running)
                                .await;
                        }
                        g.clone()
                    }
                    Ok(ApprovalDecision::ApproveWithEdits(edited)) => {
                        let _ = tx.send(CouncilEvent::PlanApproved).await;
                        if let Some(repo) = &config.repository {
                            if let Ok(json) = serde_json::to_string(&*edited) {
                                let _ = repo.update_graph(&run_id, &json).await;
                            }
                            let _ = repo
                                .update_run_status(&run_id, CouncilRunStatus::Running)
                                .await;
                        }
                        *edited
                    }
                    Ok(ApprovalDecision::Reject(reason)) => {
                        let reject_event = CouncilEvent::PlanRejected {
                            reason: Some(reason.clone()),
                        };
                        let _ = tx.send(reject_event).await;
                        if let Some(repo) = &config.repository {
                            let _ = repo
                                .update_run_status(&run_id, CouncilRunStatus::Failed)
                                .await;
                        }
                        let _ = tx
                            .send(CouncilEvent::CouncilError {
                                message: format!("plan rejected: {reason}"),
                            })
                            .await;
                        return Err(ExecuteError::PlanRejected { reason });
                    }
                    Err(_) => {
                        // Registry dropped — treat as rejection (process restart).
                        if let Some(repo) = &config.repository {
                            let _ = repo
                                .update_run_status(&run_id, CouncilRunStatus::Interrupted)
                                .await;
                        }
                        return Err(ExecuteError::PlanRejected {
                            reason: "approval channel closed".into(),
                        });
                    }
                }
            } else {
                // No registry — auto-approve.
                let _ = tx.send(CouncilEvent::PlanApproved).await;
                g.clone()
            }
        } else {
            let _ = tx.send(CouncilEvent::PlanApproved).await;
            g.clone()
        };

        approved_graph
    };

    // Persist the graph after plan approval.
    if let Some(repo) = &config.repository {
        if let Ok(json) = serde_json::to_string(&graph) {
            if let Err(e) = repo.update_graph(&run_id, &json).await {
                tracing::warn!("council: failed to persist graph: {e}");
            }
        }
    }

    // ── 2. Topological wave execution ────────────────────────────────────────
    // For resume: pre-populate `completed` with Done nodes.
    let pre_completed: HashSet<NodeId> = graph
        .nodes
        .iter()
        .filter(|(_, n)| n.status == gglib_core::domain::council::task_graph::NodeStatus::Done)
        .map(|(id, _)| id.clone())
        .collect();

    // Create the concurrency semaphore here so it is shared across all
    // recursive Team and spawn sub-team wave loops.
    let sem = Arc::new(Semaphore::new(config.max_worker_concurrency));

    let result = run_wave_loop(
        goal,
        &mut graph,
        pre_completed,
        Arc::clone(&llm),
        Arc::clone(&tool_executor),
        &config,
        &run_id,
        &mut event_seq,
        &tx,
        Arc::clone(&sem),
        0,
    )
    .await;

    match result {
        Ok(compacted) => {
            if let Some(repo) = &config.repository {
                if let Ok(json) = serde_json::to_string(&graph) {
                    let _ = repo.update_graph(&run_id, &json).await;
                }
                let _ = repo
                    .update_run_status(&run_id, CouncilRunStatus::Completed)
                    .await;
            }
            // ── 3. Synthesis ─────────────────────────────────────────────────
            run_synthesis(&graph, &compacted, &llm, &tool_executor, &tx).await;
            Ok(())
        }
        Err(e) => {
            if let Some(repo) = &config.repository {
                let _ = repo
                    .update_run_status(&run_id, CouncilRunStatus::Failed)
                    .await;
            }
            let _ = tx
                .send(CouncilEvent::CouncilError {
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
    graph: &mut TaskGraph,
    mut completed: HashSet<NodeId>,
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    config: &CouncilConfig,
    run_id: &str,
    event_seq: &mut i64,
    tx: &mpsc::Sender<CouncilEvent>,
    sem: Arc<Semaphore>,
    depth: u32,
) -> Result<HashMap<NodeId, String>, ExecuteError> {
    debug_assert!(
        depth < config.max_team_depth,
        "council: team recursion depth {depth} reached limit {}",
        config.max_team_depth
    );
    if depth >= config.max_team_depth {
        return Err(ExecuteError::MaxDepthExceeded);
    }
    let mut compacted: HashMap<NodeId, String> = HashMap::new();
    // Phase M: start from the rewind wave index at depth 0 so events carry
    // the correct wave number when re-executing after a rewind.
    let mut wave_number: u32 = if depth == 0 {
        config.rewind_to_wave.map_or(0, |w| w + 1)
    } else {
        0
    };

    loop {
        // ── Steering note drain (depth 0 only, before each wave) ─────────────
        if depth == 0 {
            if let Some(queue) = &config.note_queue {
                let notes: Vec<String> = {
                    let mut q = queue.lock().await;
                    std::mem::take(&mut *q)
                };
                for note in notes {
                    match super::steering::steering_call(graph, &note, &llm).await {
                        Ok(diff) => match graph.apply_diff(&diff) {
                            Ok(()) => {
                                let _ = tx
                                    .send(CouncilEvent::SteeringApplied {
                                        diff,
                                        applied_at_wave: wave_number,
                                    })
                                    .await;
                            }
                            Err(e) => {
                                tracing::warn!("council: steering diff invalid, skipping: {e}");
                            }
                        },
                        Err(e) => {
                            tracing::warn!("council: steering call failed, skipping: {e}");
                        }
                    }
                }
            }
        }

        let ready: Vec<NodeId> = graph.ready_nodes(&completed).into_iter().cloned().collect();
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
                    let event = CouncilEvent::AwaitingApproval {
                        approval_id: approval_id.clone(),
                        kind: ApprovalKind::Node {
                            node_id: node_id.0.clone(),
                        },
                    };
                    let _ = tx.send(event.clone()).await;
                    if let Some(repo) = &config.repository {
                        let ev = CouncilRunEvent {
                            run_id: run_id.to_string(),
                            seq: *event_seq,
                            event_json: serde_json::to_string(&event).unwrap_or_default(),
                            created_at: chrono::Utc::now().to_rfc3339(),
                            wave_index: wave_number,
                        };
                        *event_seq += 1;
                        if let Err(e) = repo.append_event(ev).await {
                            tracing::warn!("council: persist event error: {e}");
                        }
                        if let Err(e) = repo
                            .update_run_status(run_id, CouncilRunStatus::AwaitingApproval)
                            .await
                        {
                            tracing::warn!("council: status update error: {e}");
                        }
                    }

                    match rx_approval.await {
                        Ok(ApprovalDecision::Approve | ApprovalDecision::ApproveWithEdits(_)) => {
                            if let Some(repo) = &config.repository {
                                let _ = repo
                                    .update_run_status(run_id, CouncilRunStatus::Running)
                                    .await;
                            }
                        }
                        Ok(ApprovalDecision::Reject(reason)) => {
                            let id_str = node_id.0.clone();
                            let _ = tx
                                .send(CouncilEvent::NodeFailed {
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
        let mut join_set: JoinSet<(
            NodeId,
            Result<(String, String), ExecuteError>,
            SpawnSink,
        )> = JoinSet::new();

        // ── Handle Team nodes inline (not spawned) ───────────────────────────
        // Team and Debate nodes must be run before we spawn the rest of the wave
        // because they are not `Send` (they borrow `event_seq`).  In practice this
        // means team/debate nodes in the same wave run serially before leaf nodes.
        let mut team_nodes_in_wave: Vec<NodeId> = Vec::new();
        let mut debate_nodes_in_wave: Vec<NodeId> = Vec::new();
        let mut leaf_nodes_in_wave: Vec<NodeId> = Vec::new();
        for node_id in &ready {
            match &graph.nodes[node_id].kind {
                TaskNodeKind::Team { .. } => team_nodes_in_wave.push(node_id.clone()),
                TaskNodeKind::Debate { .. } => debate_nodes_in_wave.push(node_id.clone()),
                TaskNodeKind::Leaf => leaf_nodes_in_wave.push(node_id.clone()),
            }
        }

        for team_id in &team_nodes_in_wave {
            // Clone the node data to release the immutable borrow before the
            // recursive mutable call.
            let (team_goal, team_role, mut sub_graph) = {
                let node = &graph.nodes[team_id];
                let sub = match &node.kind {
                    TaskNodeKind::Team { subgraph } => (**subgraph).clone(),
                    TaskNodeKind::Leaf | TaskNodeKind::Debate { .. } => {
                        unreachable!("expected Team node")
                    }
                };
                (node.goal.clone(), node.role.clone(), sub)
            };

            // Emit TeamStarted before recursing.
            let _ = tx
                .send(CouncilEvent::TeamStarted {
                    team_id: team_id.0.clone(),
                    role: team_role,
                })
                .await;

            let sub_result = Box::pin(run_wave_loop(
                &team_goal,
                &mut sub_graph,
                HashSet::new(),
                Arc::clone(&llm),
                Arc::clone(&tool_executor),
                config,
                run_id,
                event_seq,
                tx,
                Arc::clone(&sem),
                depth + 1,
            ))
            .await;

            match sub_result {
                Ok(sub_compacted) => {
                    let summary = sub_compacted
                        .values()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    // Emit TeamSynthesized with the compacted summary.
                    let _ = tx
                        .send(CouncilEvent::TeamSynthesized {
                            team_id: team_id.0.clone(),
                            compacted_output: summary.clone(),
                        })
                        .await;
                    compacted.insert(team_id.clone(), summary);
                    completed.insert(team_id.clone());
                }
                Err(e) => return Err(e),
            }
        }

        // ── Debate nodes (Phase 3 — full dispatch via debate::run_debate_node) ─
        // Debate nodes are spawned into the same JoinSet as leaf nodes so their
        // LLM calls can overlap with leaf tool-I/O stages.  Each debate node
        // gets its own fresh CancellationToken; external cancellation (Phase K+)
        // will wire into this later.
        for debate_id in &debate_nodes_in_wave {
            let node = &graph.nodes[debate_id];
            let node_id_clone = debate_id.clone();
            let node_goal = node.goal.clone();

            // Extract the DebateConfig from the node kind.
            let debate_cfg = match &node.kind {
                TaskNodeKind::Debate { config } => config.clone(),
                _ => unreachable!("expected Debate node"),
            };

            let predecessor_context = build_predecessor_context(debate_id, graph, &compacted);

            let sem_clone = Arc::clone(&sem);
            let llm_clone = Arc::clone(&llm);
            let tool_executor_clone = Arc::clone(&tool_executor);
            let tx_clone = tx.clone();

            // Debate nodes don't use SpawnSink — create an inert one.
            let spawn_sink: SpawnSink = Arc::new(tokio::sync::Mutex::new(None));

            join_set.spawn(async move {
                let _permit = sem_clone
                    .acquire_owned()
                    .await
                    .expect("semaphore not closed");

                let result = run_debate_worker(
                    &node_id_clone,
                    &node_goal,
                    predecessor_context,
                    debate_cfg,
                    llm_clone,
                    tool_executor_clone,
                    &tx_clone,
                )
                .await;

                (node_id_clone, result, spawn_sink)
            });
        }

        // Spawn all ready *leaf* nodes concurrently, bounded by the semaphore.
        for node_id in &leaf_nodes_in_wave {
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

            // Each leaf gets its own SpawnSink so a worker can request a sub-team.
            let spawn_sink: SpawnSink = Arc::new(tokio::sync::Mutex::new(None));
            let spawn_sink_for_spawn = Arc::clone(&spawn_sink);

            join_set.spawn(async move {
                let _permit = sem_clone
                    .acquire_owned()
                    .await
                    .expect("semaphore not closed");

                // Wrap the executor to intercept spawn_subteam calls.
                let capturing_executor: Arc<dyn ToolExecutorPort> = Arc::new(
                    SpawnCapturingExecutor::new(tool_executor_clone, spawn_sink_for_spawn),
                );

                let result = run_worker(
                    &node_id_clone,
                    &node_goal,
                    predecessor_context,
                    allowlist,
                    llm_clone,
                    capturing_executor,
                    &tx_clone,
                )
                .await;

                (node_id_clone, result, spawn_sink)
            });
        }

        // Collect results; fail fast on first error.
        // Defer spawn processing until after the full wave completes so that
        // parallel workers don't interleave with spawn approval/planning.
        let mut pending_spawns: Vec<(NodeId, String, SpawnRequest)> = Vec::new();

        while let Some(join_result) = join_set.join_next().await {
            let (node_id, worker_result, sink) = join_result.expect("join_set task panicked");

            match worker_result {
                Ok((full_output, compacted_output)) => {
                    // Check whether the worker requested a spawn.
                    let maybe_spawn = sink.lock().await.take();
                    if let Some(req) = maybe_spawn {
                        pending_spawns.push((node_id.clone(), full_output, req));
                    }
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

        // ── Process spawn requests (serially after wave) ─────────────────────
        for (parent_node_id, _parent_output, spawn_req) in pending_spawns {
            // Determine whether a human needs to approve the spawn.
            let approved = if config.hitl_mode >= HitlMode::ApproveEachNode {
                if let Some(registry) = &config.approval_registry {
                    let approval_id = uuid::Uuid::new_v4().to_string();
                    let (tx_approval, rx_approval) =
                        tokio::sync::oneshot::channel::<ApprovalDecision>();
                    registry.register(approval_id.clone(), tx_approval);

                    let event = CouncilEvent::AwaitingApproval {
                        approval_id: approval_id.clone(),
                        kind: gglib_core::domain::council::events::ApprovalKind::SpawnSubteam {
                            node_id: parent_node_id.0.clone(),
                            suggested_roles: spawn_req.suggested_roles.clone(),
                        },
                    };
                    let _ = tx.send(event).await;

                    match rx_approval.await {
                        Ok(ApprovalDecision::Approve | ApprovalDecision::ApproveWithEdits(_)) => {
                            true
                        }
                        Ok(ApprovalDecision::Reject(reason)) => {
                            // Rejected spawn — log a warning and skip.
                            tracing::warn!(
                                "council: spawn_subteam rejected for node '{}': {reason}",
                                parent_node_id.0
                            );
                            false
                        }
                        Err(_) => false,
                    }
                } else {
                    // Registry absent → auto-approve.
                    true
                }
            } else {
                // HitlMode::None → auto-approve.
                true
            };

            if !approved {
                continue;
            }

            // Plan the child subgraph.
            let child_graph = match plan(
                &spawn_req.goal,
                &[], // no tool list — director planning only
                Arc::clone(&llm),
                config.hitl_mode.clone(),
                config.max_replans,
                None, // no event forwarding for child planning
            )
            .await
            {
                Ok(g) => g,
                Err(e) => {
                    tracing::warn!(
                        "council: spawn_subteam planning failed for node '{}': {e}",
                        parent_node_id.0
                    );
                    continue;
                }
            };

            let child_summary_line = format!(
                "{} nodes for goal: {}",
                child_graph.nodes.len(),
                &spawn_req.goal
            );

            // Emit SubteamSpawned event.
            let _ = tx
                .send(CouncilEvent::SubteamSpawned {
                    parent_node_id: parent_node_id.0.clone(),
                    child_graph_summary: child_summary_line,
                })
                .await;

            // Run the child subgraph recursively.
            let mut child_graph_mut = child_graph;
            let child_result = Box::pin(run_wave_loop(
                &spawn_req.goal,
                &mut child_graph_mut,
                HashSet::new(),
                Arc::clone(&llm),
                Arc::clone(&tool_executor),
                config,
                run_id,
                event_seq,
                tx,
                Arc::clone(&sem),
                depth + 1,
            ))
            .await;

            match child_result {
                Ok(child_compacted) => {
                    // Merge child compacted outputs into the parent's map,
                    // prefixing with the spawn context so downstream nodes
                    // see the enriched context.
                    let child_summary = child_compacted
                        .values()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("\n");
                    // Append to the parent node's existing compacted output.
                    if let Some(existing) = compacted.get_mut(&parent_node_id) {
                        existing.push_str("\n\n[Spawned sub-team output]\n");
                        existing.push_str(&child_summary);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        // Emit WaveCompleted at depth 0 so the frontend scrubber has waypoints.
        if depth == 0 {
            let wave_completed = CouncilEvent::WaveCompleted {
                wave_index: wave_number,
                node_count: ready.len(),
            };
            let _ = tx.send(wave_completed.clone()).await;
            if let Some(repo) = &config.repository {
                let ev = CouncilRunEvent {
                    run_id: run_id.to_string(),
                    seq: *event_seq,
                    event_json: serde_json::to_string(&wave_completed).unwrap_or_default(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    wave_index: wave_number,
                };
                *event_seq += 1;
                if let Err(e) = repo.append_event(ev).await {
                    tracing::warn!("council: persist wave_completed error: {e}");
                }
            }
        }

        wave_number += 1;
    } // end loop

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
    tx: &mpsc::Sender<CouncilEvent>,
) -> Result<(String, String), ExecuteError> {
    let id_str = node_id.0.clone();

    let _ = tx
        .send(CouncilEvent::NodeStarted {
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
                    .send(CouncilEvent::NodeTextDelta {
                        node_id: id_str.clone(),
                        delta,
                    })
                    .await;
            }
            AgentEvent::ReasoningDelta { content: delta } => {
                let _ = tx
                    .send(CouncilEvent::NodeReasoningDelta {
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
                    .send(CouncilEvent::NodeProgress {
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
                    .send(CouncilEvent::NodeToolCallStart {
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
                    .send(CouncilEvent::NodeToolCallComplete {
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
                    .send(CouncilEvent::NodeSystemWarning {
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
                .send(CouncilEvent::NodeFailed {
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
                .send(CouncilEvent::NodeFailed {
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
            .send(CouncilEvent::NodeFailed {
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
        .send(CouncilEvent::NodeCompacting {
            node_id: id_str.clone(),
        })
        .await;

    let compacted =
        match compact_worker_output(&id_str, node_goal, &full_output, &llm, &tool_executor).await {
            Ok(s) => s,
            Err(e) => {
                let msg = e.to_string();
                let _ = tx
                    .send(CouncilEvent::NodeFailed {
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
        .send(CouncilEvent::NodeComplete {
            node_id: id_str,
            output_preview: preview,
        })
        .await;

    Ok((full_output, compacted))
}

// =============================================================================
// Debate worker execution
// =============================================================================

/// Run a single debate node to completion and return
/// `(synthesis_text, compacted_output)` on success.
///
/// Emits [`CouncilEvent::NodeStarted`] immediately, then delegates the full
/// multi-round debate to [`debate::run_debate_node`], which emits all
/// `Debate*` events directly on `tx`.  After the debate concludes the
/// synthesis text is compacted via [`compact_worker_output`] so the DAG
/// successor nodes receive a dense predecessor context.
///
/// A fresh [`CancellationToken`] is created per debate node.  External
/// cancellation (Phase K+) will wire into this path later.
#[allow(clippy::too_many_arguments)]
async fn run_debate_worker(
    node_id: &NodeId,
    node_goal: &str,
    predecessor_context: String,
    config: gglib_core::domain::council::task_graph::DebateConfig,
    llm: Arc<dyn LlmCompletionPort>,
    tool_executor: Arc<dyn ToolExecutorPort>,
    tx: &mpsc::Sender<CouncilEvent>,
) -> Result<(String, String), ExecuteError> {
    let id_str = node_id.0.clone();

    let _ = tx
        .send(CouncilEvent::NodeStarted {
            node_id: id_str.clone(),
            goal: node_goal.to_owned(),
        })
        .await;

    // Fresh cancellation token — debate internal checks use this between turns.
    let cancel = CancellationToken::new();
    let agent_config = AgentConfig::default();

    let synthesis_text = match debate::run_debate_node(
        &id_str,
        node_goal,
        &predecessor_context,
        &config,
        Arc::clone(&llm),
        Arc::clone(&tool_executor),
        &agent_config,
        tx,
        cancel,
    )
    .await
    {
        Ok(text) => text,
        Err(e) => {
            let reason = match e {
                debate::DebateError::Cancelled => "debate cancelled".to_owned(),
                debate::DebateError::AgentFailed => "debate agent failed".to_owned(),
            };
            let _ = tx
                .send(CouncilEvent::NodeFailed {
                    node_id: id_str.clone(),
                    error: reason.clone(),
                })
                .await;
            return Err(ExecuteError::WorkerFailed {
                node_id: id_str,
                reason,
            });
        }
    };

    // Compact the synthesis text so successors get a dense predecessor block.
    let _ = tx
        .send(CouncilEvent::NodeCompacting {
            node_id: id_str.clone(),
        })
        .await;

    let compacted = match compact_worker_output(
        &id_str,
        node_goal,
        &synthesis_text,
        &llm,
        &tool_executor,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            let _ = tx
                .send(CouncilEvent::NodeFailed {
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

    let preview: String = synthesis_text.chars().take(200).collect();
    let _ = tx
        .send(CouncilEvent::NodeComplete {
            node_id: id_str,
            output_preview: preview,
        })
        .await;

    Ok((synthesis_text, compacted))
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
    use gglib_core::domain::council::task_graph::{
        HitlMode, NodeId, NodeStatus, TaskGraph, TaskNode, TaskNodeKind,
    };

    fn make_node(id: &str, deps: &[&str]) -> TaskNode {
        TaskNode {
            id: NodeId(id.into()),
            goal: id.into(),
            depends_on: deps.iter().map(|d| NodeId((*d).to_string())).collect(),
            tool_allowlist: vec![],
            kind: TaskNodeKind::Leaf,
            role: None,
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
