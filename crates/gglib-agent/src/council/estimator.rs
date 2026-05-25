//! Pure run-cost estimator for the orchestrator.
//!
//! # Cost model
//!
//! Each node in the task graph incurs an estimated **2 000 tokens** of LLM
//! traffic (system-prompt + context window + output).  Wall-clock time is
//! derived by assuming a sustained throughput of **50 tokens / second** — a
//! conservative baseline for a single llama-server instance running on
//! consumer hardware.
//!
//! ```text
//! est_tokens      = node_count × 2 000
//! est_wall_seconds = est_tokens ÷ 50
//! ```
//!
//! These numbers are intentionally approximate.  The estimator is a
//! warn-only advisory: it never blocks execution.

use gglib_core::domain::council::task_graph::TaskGraph;

// ─── Constants ────────────────────────────────────────────────────────────────

/// Estimated LLM tokens consumed per orchestrator node (system-prompt +
/// context + output).
pub const TOKENS_PER_NODE: u64 = 2_000;

/// Assumed sustained token throughput in tokens per second.
pub const TOKENS_PER_SECOND: u64 = 50;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Output of [`estimate_run_cost`].
///
/// All fields are advisory estimates — never enforced as hard limits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCostEstimate {
    /// Total aggregate node count across all subgraphs.
    pub node_count: usize,
    /// Rough token estimate (input + output) for the entire run.
    pub est_tokens: u64,
    /// Estimated wall-clock seconds at [`TOKENS_PER_SECOND`] tokens/second.
    pub est_wall_seconds: u64,
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Estimate the cost of executing `graph` without performing any I/O.
///
/// The function is pure — it reads only the structure of the task graph.
///
/// # Examples
///
/// A simple 3-node graph (e.g. two workers + one synthesizer):
///
/// ```rust
/// use gglib_agent::council::estimator::estimate_run_cost;
/// use gglib_core::domain::council::task_graph::{
///     TaskGraph, TaskNode, TaskNodeKind, NodeId, NodeStatus, HitlMode,
/// };
///
/// fn leaf(id: &str) -> TaskNode {
///     TaskNode {
///         id: NodeId(id.into()),
///         goal: id.into(),
///         depends_on: vec![],
///         tool_allowlist: vec![],
///         kind: TaskNodeKind::Leaf,
///         role: None,
///         status: NodeStatus::Pending,
///         output: None,
///         compacted_output: None,
///         error: None,
///     }
/// }
///
/// let graph = TaskGraph::new(
///     "test".into(),
///     HitlMode::None,
///     vec![leaf("a"), leaf("b"), leaf("c")],
/// )
/// .unwrap();
///
/// let est = estimate_run_cost(&graph);
/// assert_eq!(est.node_count, 3);
/// assert_eq!(est.est_tokens, 6_000);
/// assert_eq!(est.est_wall_seconds, 120);
/// ```
///
/// A graph with a Team node whose subgraph contains 7 leaves, giving a total
/// of 1 (team) + 7 (sub-leaves) + 7 (other top-level leaves) = 15 nodes:
///
/// ```rust
/// use gglib_agent::council::estimator::estimate_run_cost;
/// use gglib_core::domain::council::task_graph::{
///     TaskGraph, TaskNode, TaskNodeKind, NodeId, NodeStatus, HitlMode,
/// };
///
/// fn leaf(id: &str) -> TaskNode {
///     TaskNode {
///         id: NodeId(id.into()),
///         goal: id.into(),
///         depends_on: vec![],
///         tool_allowlist: vec![],
///         kind: TaskNodeKind::Leaf,
///         role: None,
///         status: NodeStatus::Pending,
///         output: None,
///         compacted_output: None,
///         error: None,
///     }
/// }
///
/// // Build a subgraph with 7 leaf nodes.
/// let sub_leaves: Vec<TaskNode> = (0..7).map(|i| leaf(&format!("s{i}"))).collect();
/// let subgraph = TaskGraph::new("sub".into(), HitlMode::None, sub_leaves).unwrap();
///
/// // Team node wraps the subgraph (counts as 1 + 7 = 8 total).
/// let team_node = TaskNode {
///     id: NodeId("team".into()),
///     goal: "team goal".into(),
///     depends_on: vec![],
///     tool_allowlist: vec![],
///     kind: TaskNodeKind::Team { subgraph: Box::new(subgraph) },
///     role: None,
///     status: NodeStatus::Pending,
///     output: None,
///     compacted_output: None,
///     error: None,
/// };
///
/// // Top-level: 1 team + 7 leaf siblings = 8 nodes → total_node_count = 15.
/// let mut top_nodes: Vec<TaskNode> = (0..7).map(|i| leaf(&format!("t{i}"))).collect();
/// top_nodes.push(team_node);
/// let graph = TaskGraph::new("big goal".into(), HitlMode::None, top_nodes).unwrap();
///
/// let est = estimate_run_cost(&graph);
/// assert_eq!(est.node_count, 15);
/// assert_eq!(est.est_tokens, 30_000);
/// assert_eq!(est.est_wall_seconds, 600);
/// ```
pub fn estimate_run_cost(graph: &TaskGraph) -> RunCostEstimate {
    let node_count = graph.total_node_count();
    let est_tokens = (node_count as u64).saturating_mul(TOKENS_PER_NODE);
    let est_wall_seconds = est_tokens / TOKENS_PER_SECOND;

    RunCostEstimate {
        node_count,
        est_tokens,
        est_wall_seconds,
    }
}
