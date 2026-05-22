//! Task graph types for the Director/Worker orchestrator.
//!
//! A [`TaskGraph`] is a directed acyclic graph (DAG) of [`TaskNode`]s that the
//! orchestrator executor drives to completion in topological order.  Nodes with
//! no unsatisfied dependencies run concurrently; each node executes as an
//! isolated `AgentLoop` worker with its own tool allowlist and context window.
//!
//! # Validation
//!
//! Call [`TaskGraph::new`] (preferred) or [`TaskGraph::validate_acyclic`]
//! (on a manually-constructed graph) before handing a graph to the executor.
//! Both methods check:
//!
//! - All `depends_on` ids resolve to existing nodes.
//! - The dependency edges form a true DAG (no cycles, no self-loops).
//! - Node count ≤ [`MAX_NODES`].
//! - Longest path depth ≤ [`MAX_DEPTH`].
//!
//! # Example
//!
//! ```rust
//! use std::collections::HashSet;
//! use gglib_core::domain::orchestrator::task_graph::{
//!     TaskGraph, TaskNode, NodeId, NodeStatus, HitlMode,
//! };
//!
//! let nodes = vec![
//!     TaskNode {
//!         id: NodeId("research".into()),
//!         goal: "Research the topic".into(),
//!         depends_on: vec![],
//!         tool_allowlist: vec!["web_search".into()],
//!         status: NodeStatus::Pending,
//!         output: None,
//!         compacted_output: None,
//!         error: None,
//!     },
//!     TaskNode {
//!         id: NodeId("draft".into()),
//!         goal: "Write the first draft".into(),
//!         depends_on: vec![NodeId("research".into())],
//!         tool_allowlist: vec![],
//!         status: NodeStatus::Pending,
//!         output: None,
//!         compacted_output: None,
//!         error: None,
//!     },
//! ];
//! let graph = TaskGraph::new("Write a research doc".into(), HitlMode::None, nodes).unwrap();
//! let ready = graph.ready_nodes(&HashSet::new());
//! assert_eq!(ready.len(), 1);
//! assert_eq!(ready[0], &NodeId("research".into()));
//! ```

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::domain::agent::ToolDefinition;

// =============================================================================
// Size limits
// =============================================================================

/// Maximum number of nodes a [`TaskGraph`] may contain.
///
/// Prevents unbounded context growth: each node's output is injected into
/// downstream nodes' context windows, so large graphs compound quickly.
pub const MAX_NODES: usize = 8;

/// Maximum depth (longest root-to-leaf path) a [`TaskGraph`] may have.
///
/// Keeps total latency bounded even when nodes are forced to run serially.
pub const MAX_DEPTH: usize = 3;

// =============================================================================
// NodeId
// =============================================================================

/// Opaque node identifier within a [`TaskGraph`].
///
/// Short, human-readable strings are recommended (e.g. `"research"`, `"draft"`,
/// `"review"`).  Uniqueness within a graph is enforced by [`TaskGraph::new`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

// =============================================================================
// NodeStatus
// =============================================================================

/// Lifecycle state of a single [`TaskNode`].
///
/// State transitions:
/// ```text
/// Pending → AwaitingApproval → Running → Compacting → Done
///                                      ↘ Failed
/// Pending → Skipped   (upstream failure)
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// Not yet eligible to run (one or more predecessors are incomplete).
    Pending,
    /// All predecessors are done but human approval is required before
    /// execution begins ([`HitlMode::ApproveEachNode`] or higher).
    AwaitingApproval,
    /// Currently executing (the worker `AgentLoop` is running).
    Running,
    /// Execution finished; the output is being compacted for downstream use.
    Compacting,
    /// Execution and compaction finished successfully.
    Done,
    /// Execution failed with an unrecoverable error.
    Failed,
    /// Skipped because an upstream node failed and no path to this node is
    /// viable.
    Skipped,
}

// =============================================================================
// HitlMode
// =============================================================================

/// Controls when the orchestrator executor pauses to request human approval.
///
/// Variants are ordered from least to most restrictive; each variant implies
/// all approvals required by lower variants.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HitlMode {
    /// Execute the graph without any human-in-the-loop gates.
    #[default]
    None,
    /// Pause once after the plan is produced so the user can review (and
    /// optionally edit) the full [`TaskGraph`] before execution begins.
    ApprovePlan,
    /// Pause before each node executes (implies `ApprovePlan`).
    ///
    /// Use this when the goal is sensitive and each step must be vetted
    /// individually.
    ApproveEachNode,
    /// Pause before each individual tool call a worker makes
    /// (implies `ApproveEachNode`).
    ///
    /// This is the most restrictive mode; it is primarily useful for
    /// debugging or for high-stakes automation contexts.
    ApproveTools,
}

// =============================================================================
// TaskNode
// =============================================================================

/// A single work unit in a [`TaskGraph`].
///
/// Each node is executed as an isolated `AgentLoop` worker whose context is
/// assembled from:
///
/// 1. Its own `goal` as the system instruction.
/// 2. Compacted outputs from each `depends_on` predecessor as additional
///    context messages.
/// 3. No orchestrator planning history (strict isolation between nodes).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    /// Short, unique identifier for this node within its graph.
    pub id: NodeId,
    /// One-sentence goal that the worker agent is asked to achieve.
    pub goal: String,
    /// Nodes whose outputs this node depends on.
    ///
    /// Must form a DAG when combined with all other nodes' `depends_on` lists.
    pub depends_on: Vec<NodeId>,
    /// Tool names the worker is permitted to call.
    ///
    /// An empty list means no tools are available to this worker.  Names must
    /// match entries in the runtime tool catalog.
    pub tool_allowlist: Vec<String>,
    /// Current lifecycle state (mutated by the executor as the node runs).
    pub status: NodeStatus,
    /// The worker's full output text (set after reaching [`NodeStatus::Done`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// Compressed summary of `output` passed to downstream nodes as context.
    ///
    /// Set by the compaction step immediately after execution finishes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compacted_output: Option<String>,
    /// Error message if the node reached [`NodeStatus::Failed`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// =============================================================================
// TaskGraphError
// =============================================================================

/// Error variants produced during [`TaskGraph`] construction or validation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskGraphError {
    /// Two nodes share the same [`NodeId`].
    #[error("duplicate node id: {0}")]
    DuplicateNodeId(String),

    /// A `depends_on` entry references a [`NodeId`] not present in the graph.
    #[error("node '{node}' depends on unknown id '{dep}'")]
    UnknownDependency {
        /// The node that declared the bad dependency.
        node: String,
        /// The referenced id that does not exist.
        dep: String,
    },

    /// The dependency edges contain at least one cycle.
    #[error("cycle detected involving node '{0}'")]
    Cycle(String),

    /// The graph exceeds [`MAX_NODES`].
    #[error("graph has {count} nodes; maximum is {max}")]
    TooManyNodes {
        /// Actual node count.
        count: usize,
        /// The limit ([`MAX_NODES`]).
        max: usize,
    },

    /// The longest root-to-leaf path exceeds [`MAX_DEPTH`].
    #[error("graph depth {depth} exceeds maximum {max}")]
    DepthExceeded {
        /// Computed depth.
        depth: usize,
        /// The limit ([`MAX_DEPTH`]).
        max: usize,
    },

    /// A node lists a tool not present in the provided catalog.
    #[error("node '{node}' requires unknown tool '{tool}'")]
    UnknownTool {
        /// The node with the invalid allowlist entry.
        node: String,
        /// The tool name that was not found in the catalog.
        tool: String,
    },
}

// =============================================================================
// Internal DFS helpers
// =============================================================================

const WHITE: u8 = 0; // not yet visited
const GRAY: u8 = 1; // currently on the DFS stack
const BLACK: u8 = 2; // fully processed

fn dfs_check_cycle<'a>(
    id: &'a NodeId,
    nodes: &'a HashMap<NodeId, TaskNode>,
    color: &mut HashMap<&'a NodeId, u8>,
) -> Result<(), TaskGraphError> {
    color.insert(id, GRAY);
    let node = &nodes[id];
    for dep in &node.depends_on {
        let dep_color = color.get(dep).copied().unwrap_or(WHITE);
        match dep_color {
            GRAY => return Err(TaskGraphError::Cycle(dep.0.clone())),
            BLACK => {} // already fully explored
            _ => dfs_check_cycle(dep, nodes, color)?,
        }
    }
    color.insert(id, BLACK);
    Ok(())
}

fn compute_depth(
    id: &NodeId,
    nodes: &HashMap<NodeId, TaskNode>,
    memo: &mut HashMap<NodeId, usize>,
) -> usize {
    if let Some(&d) = memo.get(id) {
        return d;
    }
    let depth = nodes[id]
        .depends_on
        .iter()
        .map(|dep| compute_depth(dep, nodes, memo) + 1)
        .max()
        .unwrap_or(0);
    memo.insert(id.clone(), depth);
    depth
}

// =============================================================================
// TaskGraph
// =============================================================================

/// A validated directed acyclic graph of [`TaskNode`]s.
///
/// Produced by the director agent via [`crate::domain::orchestrator::events::OrchestratorEvent::PlanProposed`]
/// and executed by the orchestrator runner in topological order.
///
/// # Construction
///
/// Prefer [`TaskGraph::new`] over direct struct construction to get automatic
/// validation.  Deserializing from a director's JSON response should be
/// followed by [`TaskGraph::validate_acyclic`] to re-check invariants.
///
/// # Concurrency model
///
/// The executor calls [`TaskGraph::ready_nodes`] after each node completes to
/// discover newly-eligible nodes, which it launches concurrently.
///
/// # Example
///
/// ```rust
/// use gglib_core::domain::orchestrator::task_graph::{TaskGraph, TaskNode, NodeId, NodeStatus, HitlMode};
///
/// let nodes = vec![TaskNode {
///     id: NodeId("only".into()),
///     goal: "Do the thing".into(),
///     depends_on: vec![],
///     tool_allowlist: vec![],
///     status: NodeStatus::Pending,
///     output: None,
///     compacted_output: None,
///     error: None,
/// }];
/// let g = TaskGraph::new("My goal".into(), HitlMode::None, nodes).unwrap();
/// assert_eq!(g.roots().len(), 1);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskGraph {
    /// The high-level goal the director is trying to achieve.
    pub goal: String,
    /// Human-in-the-loop approval policy for this execution run.
    pub hitl_mode: HitlMode,
    /// All work units, keyed by their unique [`NodeId`].
    pub nodes: HashMap<NodeId, TaskNode>,
}

impl TaskGraph {
    /// Construct and validate a [`TaskGraph`] from a flat list of nodes.
    ///
    /// Returns an error if the node list violates any structural invariant
    /// (duplicate ids, unknown dependencies, cycles, size/depth limits).
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::orchestrator::task_graph::{
    ///     TaskGraph, TaskNode, NodeId, NodeStatus, HitlMode,
    /// };
    ///
    /// let nodes = vec![TaskNode {
    ///     id: NodeId("only".into()),
    ///     goal: "Do the thing".into(),
    ///     depends_on: vec![],
    ///     tool_allowlist: vec![],
    ///     status: NodeStatus::Pending,
    ///     output: None,
    ///     compacted_output: None,
    ///     error: None,
    /// }];
    /// let g = TaskGraph::new("Top goal".into(), HitlMode::None, nodes);
    /// assert!(g.is_ok());
    /// ```
    pub fn new(
        goal: String,
        hitl_mode: HitlMode,
        nodes: Vec<TaskNode>,
    ) -> Result<Self, TaskGraphError> {
        if nodes.len() > MAX_NODES {
            return Err(TaskGraphError::TooManyNodes {
                count: nodes.len(),
                max: MAX_NODES,
            });
        }

        let mut map: HashMap<NodeId, TaskNode> = HashMap::with_capacity(nodes.len());
        for node in nodes {
            if map.contains_key(&node.id) {
                return Err(TaskGraphError::DuplicateNodeId(node.id.0));
            }
            map.insert(node.id.clone(), node);
        }

        let graph = Self {
            goal,
            hitl_mode,
            nodes: map,
        };
        graph.validate_acyclic()?;
        Ok(graph)
    }

    /// Validate that the dependency graph contains no cycles.
    ///
    /// Uses DFS colouring (white → gray → black).  Also validates that all
    /// `depends_on` ids resolve to existing nodes and that depth ≤ [`MAX_DEPTH`].
    ///
    /// # Errors
    ///
    /// Returns the first structural violation found.
    ///
    /// # Example — detecting a cycle
    ///
    /// ```rust
    /// use std::collections::HashMap;
    /// use gglib_core::domain::orchestrator::task_graph::{
    ///     TaskGraph, TaskNode, NodeId, NodeStatus, HitlMode, TaskGraphError,
    /// };
    ///
    /// let mut nodes = HashMap::new();
    /// for (id, dep) in [("a", "b"), ("b", "a")] {
    ///     nodes.insert(NodeId(id.into()), TaskNode {
    ///         id: NodeId(id.into()),
    ///         goal: id.into(),
    ///         depends_on: vec![NodeId(dep.into())],
    ///         tool_allowlist: vec![],
    ///         status: NodeStatus::Pending,
    ///         output: None, compacted_output: None, error: None,
    ///     });
    /// }
    /// let g = TaskGraph { goal: "cyclic".into(), hitl_mode: HitlMode::None, nodes };
    /// assert!(matches!(g.validate_acyclic(), Err(TaskGraphError::Cycle(_))));
    /// ```
    pub fn validate_acyclic(&self) -> Result<(), TaskGraphError> {
        // Check all depends_on ids resolve.
        for (id, node) in &self.nodes {
            for dep in &node.depends_on {
                if !self.nodes.contains_key(dep) {
                    return Err(TaskGraphError::UnknownDependency {
                        node: id.0.clone(),
                        dep: dep.0.clone(),
                    });
                }
            }
        }

        // DFS cycle detection.
        let mut color: HashMap<&NodeId, u8> = HashMap::new();
        for id in self.nodes.keys() {
            if color.get(id).copied().unwrap_or(WHITE) == WHITE {
                dfs_check_cycle(id, &self.nodes, &mut color)?;
            }
        }

        // Depth check.
        let mut memo: HashMap<NodeId, usize> = HashMap::new();
        for id in self.nodes.keys() {
            let depth = compute_depth(id, &self.nodes, &mut memo);
            if depth > MAX_DEPTH {
                return Err(TaskGraphError::DepthExceeded {
                    depth,
                    max: MAX_DEPTH,
                });
            }
        }

        Ok(())
    }

    /// Validate that every tool name in every node's `tool_allowlist` exists
    /// in `catalog`.
    ///
    /// Call this after [`TaskGraph::validate_acyclic`] when the runtime tool
    /// catalog is available.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::orchestrator::task_graph::{
    ///     TaskGraph, TaskNode, NodeId, NodeStatus, HitlMode, TaskGraphError,
    /// };
    /// use gglib_core::ToolDefinition;
    ///
    /// let catalog = vec![ToolDefinition {
    ///     name: "web_search".into(),
    ///     description: None,
    ///     input_schema: None,
    ///     title: None,
    /// }];
    /// let nodes = vec![TaskNode {
    ///     id: NodeId("r".into()),
    ///     goal: "research".into(),
    ///     depends_on: vec![],
    ///     tool_allowlist: vec!["nonexistent".into()],
    ///     status: NodeStatus::Pending,
    ///     output: None, compacted_output: None, error: None,
    /// }];
    /// let g = TaskGraph::new("goal".into(), HitlMode::None, nodes).unwrap();
    /// assert!(matches!(
    ///     g.validate_tool_allowlist(&catalog),
    ///     Err(TaskGraphError::UnknownTool { .. })
    /// ));
    /// ```
    pub fn validate_tool_allowlist(
        &self,
        catalog: &[ToolDefinition],
    ) -> Result<(), TaskGraphError> {
        let known: HashSet<&str> = catalog.iter().map(|t| t.name.as_str()).collect();
        for (id, node) in &self.nodes {
            for tool in &node.tool_allowlist {
                if !known.contains(tool.as_str()) {
                    return Err(TaskGraphError::UnknownTool {
                        node: id.0.clone(),
                        tool: tool.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Return the ids of root nodes — those with an empty `depends_on` list.
    ///
    /// # Example
    ///
    /// ```rust
    /// use gglib_core::domain::orchestrator::task_graph::{
    ///     TaskGraph, TaskNode, NodeId, NodeStatus, HitlMode,
    /// };
    ///
    /// let nodes = vec![
    ///     TaskNode { id: NodeId("root".into()), goal: "g".into(), depends_on: vec![],
    ///                tool_allowlist: vec![], status: NodeStatus::Pending,
    ///                output: None, compacted_output: None, error: None },
    ///     TaskNode { id: NodeId("child".into()), goal: "g".into(),
    ///                depends_on: vec![NodeId("root".into())],
    ///                tool_allowlist: vec![], status: NodeStatus::Pending,
    ///                output: None, compacted_output: None, error: None },
    /// ];
    /// let g = TaskGraph::new("goal".into(), HitlMode::None, nodes).unwrap();
    /// assert_eq!(g.roots().len(), 1);
    /// assert_eq!(g.roots()[0], &NodeId("root".into()));
    /// ```
    pub fn roots(&self) -> Vec<&NodeId> {
        let mut roots: Vec<&NodeId> = self
            .nodes
            .keys()
            .filter(|id| self.nodes[*id].depends_on.is_empty())
            .collect();
        // Sort for deterministic ordering.
        roots.sort_by(|a, b| a.0.cmp(&b.0));
        roots
    }

    /// Return the ids of nodes eligible to run given the set of
    /// already-completed nodes.
    ///
    /// A node is eligible when:
    /// - It is **not** in `completed`.
    /// - All of its `depends_on` predecessors **are** in `completed`.
    ///
    /// The returned slice is sorted by node id for deterministic ordering.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::collections::HashSet;
    /// use gglib_core::domain::orchestrator::task_graph::{
    ///     TaskGraph, TaskNode, NodeId, NodeStatus, HitlMode,
    /// };
    ///
    /// let nodes = vec![
    ///     TaskNode { id: NodeId("a".into()), goal: "g".into(), depends_on: vec![],
    ///                tool_allowlist: vec![], status: NodeStatus::Pending,
    ///                output: None, compacted_output: None, error: None },
    ///     TaskNode { id: NodeId("b".into()), goal: "g".into(),
    ///                depends_on: vec![NodeId("a".into())],
    ///                tool_allowlist: vec![], status: NodeStatus::Pending,
    ///                output: None, compacted_output: None, error: None },
    /// ];
    /// let g = TaskGraph::new("goal".into(), HitlMode::None, nodes).unwrap();
    ///
    /// // Nothing completed — only the root is ready.
    /// let ready = g.ready_nodes(&HashSet::new());
    /// assert_eq!(ready.len(), 1);
    /// assert_eq!(ready[0], &NodeId("a".into()));
    ///
    /// // "a" completed — "b" is now ready.
    /// let done: HashSet<NodeId> = [NodeId("a".into())].into();
    /// let ready = g.ready_nodes(&done);
    /// assert_eq!(ready.len(), 1);
    /// assert_eq!(ready[0], &NodeId("b".into()));
    /// ```
    pub fn ready_nodes(&self, completed: &HashSet<NodeId>) -> Vec<&NodeId> {
        let mut ready: Vec<&NodeId> = self
            .nodes
            .keys()
            .filter(|id| {
                !completed.contains(*id)
                    && self.nodes[*id]
                        .depends_on
                        .iter()
                        .all(|dep| completed.contains(dep))
            })
            .collect();
        // Sort for deterministic ordering.
        ready.sort_by(|a, b| a.0.cmp(&b.0));
        ready
    }
}

// =============================================================================
// Unit tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn leaf(id: &str) -> TaskNode {
        TaskNode {
            id: NodeId(id.into()),
            goal: id.into(),
            depends_on: vec![],
            tool_allowlist: vec![],
            status: NodeStatus::Pending,
            output: None,
            compacted_output: None,
            error: None,
        }
    }

    fn node(id: &str, deps: &[&str]) -> TaskNode {
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

    fn ids(v: Vec<&NodeId>) -> Vec<&str> {
        v.into_iter().map(|id| id.0.as_str()).collect()
    }

    // ------------------------------------------------------------------
    // validate_acyclic — valid graphs
    // ------------------------------------------------------------------

    #[test]
    fn empty_graph_is_valid() {
        let g = TaskGraph {
            goal: "g".into(),
            hitl_mode: HitlMode::None,
            nodes: HashMap::new(),
        };
        assert!(g.validate_acyclic().is_ok());
    }

    #[test]
    fn single_node_is_valid() {
        let g = TaskGraph::new("g".into(), HitlMode::None, vec![leaf("a")]).unwrap();
        assert!(g.validate_acyclic().is_ok());
    }

    #[test]
    fn linear_chain_is_valid() {
        // a → b → c
        let g = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![leaf("a"), node("b", &["a"]), node("c", &["b"])],
        )
        .unwrap();
        assert!(g.validate_acyclic().is_ok());
    }

    #[test]
    fn diamond_shape_is_valid() {
        // root → (left, right) → merge
        let g = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![
                leaf("root"),
                node("left", &["root"]),
                node("right", &["root"]),
                node("merge", &["left", "right"]),
            ],
        )
        .unwrap();
        assert!(g.validate_acyclic().is_ok());
    }

    // ------------------------------------------------------------------
    // validate_acyclic — invalid graphs
    // ------------------------------------------------------------------

    #[test]
    fn simple_cycle_is_rejected() {
        // a → b → a
        let nodes = vec![node("a", &["b"]), node("b", &["a"])];
        let g = TaskGraph {
            goal: "g".into(),
            hitl_mode: HitlMode::None,
            nodes: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
        };
        assert!(matches!(
            g.validate_acyclic(),
            Err(TaskGraphError::Cycle(_))
        ));
    }

    #[test]
    fn self_loop_is_rejected() {
        let nodes = vec![node("a", &["a"])];
        let g = TaskGraph {
            goal: "g".into(),
            hitl_mode: HitlMode::None,
            nodes: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
        };
        assert!(matches!(
            g.validate_acyclic(),
            Err(TaskGraphError::Cycle(_))
        ));
    }

    #[test]
    fn unknown_dependency_is_rejected() {
        let nodes = vec![node("a", &["nonexistent"])];
        let g = TaskGraph {
            goal: "g".into(),
            hitl_mode: HitlMode::None,
            nodes: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
        };
        assert!(matches!(
            g.validate_acyclic(),
            Err(TaskGraphError::UnknownDependency { .. })
        ));
    }

    #[test]
    fn too_many_nodes_is_rejected() {
        let nodes: Vec<TaskNode> = (0..=MAX_NODES).map(|i| leaf(&i.to_string())).collect();
        assert!(matches!(
            TaskGraph::new("g".into(), HitlMode::None, nodes),
            Err(TaskGraphError::TooManyNodes { .. })
        ));
    }

    #[test]
    fn duplicate_node_id_is_rejected() {
        let nodes = vec![leaf("a"), leaf("a")];
        assert!(matches!(
            TaskGraph::new("g".into(), HitlMode::None, nodes),
            Err(TaskGraphError::DuplicateNodeId(_))
        ));
    }

    #[test]
    fn depth_exceeded_is_rejected() {
        // Chain of MAX_DEPTH + 2 nodes creates a path of depth MAX_DEPTH + 1.
        let mut nodes = vec![leaf("0")];
        for i in 1..=(MAX_DEPTH + 1) {
            nodes.push(node(&i.to_string(), &[&(i - 1).to_string()]));
        }
        assert!(matches!(
            TaskGraph::new("g".into(), HitlMode::None, nodes),
            Err(TaskGraphError::DepthExceeded { .. })
        ));
    }

    // ------------------------------------------------------------------
    // roots
    // ------------------------------------------------------------------

    #[test]
    fn roots_returns_nodes_with_no_deps() {
        // Diamond: root → (left, right) → merge.  Only root has no deps.
        let g = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![
                leaf("root"),
                node("left", &["root"]),
                node("right", &["root"]),
                node("merge", &["left", "right"]),
            ],
        )
        .unwrap();
        assert_eq!(ids(g.roots()), vec!["root"]);
    }

    #[test]
    fn roots_returns_all_when_no_deps() {
        let g = TaskGraph::new("g".into(), HitlMode::None, vec![leaf("a"), leaf("b")]).unwrap();
        // Two independent nodes — both are roots (sorted).
        assert_eq!(ids(g.roots()), vec!["a", "b"]);
    }

    // ------------------------------------------------------------------
    // ready_nodes
    // ------------------------------------------------------------------

    #[test]
    fn ready_nodes_returns_roots_when_nothing_completed() {
        let g = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![leaf("a"), node("b", &["a"])],
        )
        .unwrap();
        let ready = ids(g.ready_nodes(&HashSet::new()));
        assert_eq!(ready, vec!["a"]);
    }

    #[test]
    fn ready_nodes_unlocks_children_after_parent_completes() {
        // a → b → c
        let g = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![leaf("a"), node("b", &["a"]), node("c", &["b"])],
        )
        .unwrap();

        let done: HashSet<NodeId> = [NodeId("a".into())].into();
        let ready = ids(g.ready_nodes(&done));
        assert_eq!(ready, vec!["b"]);
    }

    #[test]
    fn ready_nodes_requires_all_deps_to_complete() {
        // a, b → merge
        let g = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![leaf("a"), leaf("b"), node("merge", &["a", "b"])],
        )
        .unwrap();

        // Only "a" done — "merge" still blocked, "b" is newly ready.
        let done: HashSet<NodeId> = [NodeId("a".into())].into();
        let ready = ids(g.ready_nodes(&done));
        assert_eq!(ready, vec!["b"]);

        // Both done — "merge" is ready.
        let done: HashSet<NodeId> = [NodeId("a".into()), NodeId("b".into())].into();
        let ready = ids(g.ready_nodes(&done));
        assert_eq!(ready, vec!["merge"]);
    }

    #[test]
    fn ready_nodes_returns_empty_when_all_completed() {
        let g = TaskGraph::new(
            "g".into(),
            HitlMode::None,
            vec![leaf("a"), node("b", &["a"])],
        )
        .unwrap();
        let done: HashSet<NodeId> = [NodeId("a".into()), NodeId("b".into())].into();
        assert!(g.ready_nodes(&done).is_empty());
    }

    // ------------------------------------------------------------------
    // validate_tool_allowlist
    // ------------------------------------------------------------------

    #[test]
    fn validate_tool_allowlist_passes_when_all_tools_known() {
        let catalog = vec![ToolDefinition {
            name: "search".into(),
            description: None,
            input_schema: None,
            title: None,
        }];
        let mut n = leaf("a");
        n.tool_allowlist = vec!["search".into()];
        let g = TaskGraph::new("g".into(), HitlMode::None, vec![n]).unwrap();
        assert!(g.validate_tool_allowlist(&catalog).is_ok());
    }

    #[test]
    fn validate_tool_allowlist_rejects_unknown_tool() {
        let catalog = vec![ToolDefinition {
            name: "search".into(),
            description: None,
            input_schema: None,
            title: None,
        }];
        let mut n = leaf("a");
        n.tool_allowlist = vec!["unknown_tool".into()];
        let g = TaskGraph::new("g".into(), HitlMode::None, vec![n]).unwrap();
        assert!(matches!(
            g.validate_tool_allowlist(&catalog),
            Err(TaskGraphError::UnknownTool { .. })
        ));
    }
}
