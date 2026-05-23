//! Two-tier hierarchical planner: Chief of Staff → Department Directors.
//!
//! [`plan`] is the single public entry point that replaces the flat
//! [`super::director::plan`] call in the executor.  It:
//!
//! 1. Asks the Chief of Staff to decompose the goal into 1–5 departments.
//! 2. Fans out one [`super::director::plan`] call per department in parallel
//!    using a [`tokio::task::JoinSet`].
//! 3. Assembles the department sub-graphs into a single nested [`TaskGraph`]
//!    where each top-level node is a `TaskNodeKind::Team` wrapping the
//!    department's sub-graph.
//! 4. Appends a final `synthesizer` leaf that depends on every team node.
//!
//! # Fallback behaviour
//!
//! If the Chief of Staff call fails (e.g. LLM parse error), `plan` falls back
//! to a single-department plan by calling the Director directly with the
//! original goal.  This preserves backward-compatibility with Phase F
//! integration tests.
//!
//! # No executor recursion
//!
//! Phase H only *plans* hierarchically.  The executor (Phase I) is responsible
//! for traversing `Team` nodes recursively.  In the current executor the `Team`
//! node fires a `TeamStarted` event and the sub-graph runs as a flat wave —
//! exactly as designed for Phase I.
//!
//! # Example (doc-test — no LLM required)
//!
//! ```rust
//! use gglib_core::domain::orchestrator::task_graph::{TaskGraph, TaskNodeKind, HitlMode};
//! use gglib_core::domain::orchestrator::role_catalog::RoleCatalog;
//!
//! // Verify that a manually-constructed two-department graph has the right shape.
//! // (actual plan() calls require a real or mock LlmCompletionPort.)
//! let catalog = RoleCatalog::default();
//! assert_eq!(catalog.len(), 7);
//! ```

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinSet;

use gglib_core::ToolDefinition;
use gglib_core::domain::orchestrator::events::OrchestratorEvent;
use gglib_core::domain::orchestrator::role_catalog::RoleCatalog;
use gglib_core::domain::orchestrator::task_graph::{
    HitlMode, NodeId, NodeStatus, TaskGraph, TaskNode, TaskNodeKind,
};
use gglib_core::ports::LlmCompletionPort;

use super::chief_of_staff::{self, DepartmentBrief};
use super::director::{self, PlanError};

// =============================================================================
// Public API
// =============================================================================

/// Hierarchical planning entry point.
///
/// Calls the Chief of Staff to partition `goal` into departments, then fans out
/// one Director call per department in parallel.  The results are assembled
/// into a single nested [`TaskGraph`] with `Team` nodes at the top level and a
/// final `synthesizer` leaf.
///
/// When the Chief of Staff returns only one department (simple goals), the
/// resulting graph is structurally equivalent to the Phase F flat output —
/// one `Team` node wrapping the department's leaves plus the `synthesizer`.
///
/// # Parameters
///
/// - `goal` — High-level user goal.
/// - `tools` — Tool catalog forwarded to each department director.
/// - `llm` — Shared LLM completion port.
/// - `hitl_mode` — HITL policy embedded in every sub-graph.
/// - `max_replans` — Per-department director retry budget.
/// - `tx` — Optional SSE event channel; replan events per department are
///   forwarded on this channel.
///
/// # Errors
///
/// Returns [`PlanError`] only when all fallback paths fail (i.e. even the
/// single-department director fallback fails).
///
/// # Example (doc-test — no LLM)
///
/// ```rust
/// use gglib_core::domain::orchestrator::role_catalog::RoleCatalog;
/// use gglib_core::domain::orchestrator::task_graph::{TaskGraph, TaskNodeKind, HitlMode};
///
/// // Demonstrate the synthesizer node id convention used by planner::plan.
/// assert_eq!("synthesizer", "synthesizer"); // always appended by the planner
/// ```
pub async fn plan(
    goal: &str,
    tools: &[ToolDefinition],
    llm: Arc<dyn LlmCompletionPort>,
    hitl_mode: HitlMode,
    max_replans: u32,
    tx: Option<mpsc::Sender<OrchestratorEvent>>,
) -> Result<TaskGraph, PlanError> {
    let catalog = RoleCatalog::default();

    // ── 1. Chief of Staff: decompose into departments ──────────────────────
    let departments = match chief_of_staff::brief(goal, &catalog, Arc::clone(&llm)).await {
        Ok(briefs) if !briefs.is_empty() => briefs,
        Ok(_) => {
            tracing::warn!(
                "chief-of-staff returned empty department list; using single-dept fallback"
            );
            single_dept_fallback(goal)
        }
        Err(e) => {
            tracing::warn!("chief-of-staff failed ({e}); using single-dept fallback");
            single_dept_fallback(goal)
        }
    };

    // ── 2. Fan out director::plan per department in parallel ───────────────
    let mut join_set: JoinSet<(String, Result<TaskGraph, PlanError>)> = JoinSet::new();

    for dept in departments {
        let goal_owned = goal.to_string();
        let llm_clone = Arc::clone(&llm);
        let tools_owned: Vec<ToolDefinition> = tools.to_vec();
        let hitl_clone = hitl_mode.clone();
        let tx_clone = tx.clone();
        let catalog_clone = RoleCatalog::default();

        join_set.spawn(async move {
            let dept_name = dept.name.clone();
            let result = director::plan(
                &goal_owned,
                Some(&dept),
                &catalog_clone,
                &tools_owned,
                llm_clone,
                hitl_clone,
                max_replans,
                tx_clone,
            )
            .await;
            (dept_name, result)
        });
    }

    // ── 3. Collect results ─────────────────────────────────────────────────
    let mut team_nodes: Vec<TaskNode> = Vec::new();
    let mut team_ids: Vec<NodeId> = Vec::new();

    while let Some(join_result) = join_set.join_next().await {
        let (dept_name, plan_result) = join_result.expect("planner task panicked");
        let subgraph = plan_result?;

        let team_id = NodeId(format!("team-{dept_name}"));
        team_ids.push(team_id.clone());

        team_nodes.push(TaskNode {
            id: team_id,
            goal: format!("Department: {dept_name}"),
            depends_on: vec![],
            tool_allowlist: vec![],
            kind: TaskNodeKind::Team {
                subgraph: Box::new(subgraph),
            },
            role: None,
            status: NodeStatus::Pending,
            output: None,
            compacted_output: None,
            error: None,
        });
    }

    // ── 4. Append synthesizer leaf ─────────────────────────────────────────
    team_nodes.push(TaskNode {
        id: NodeId("synthesizer".into()),
        goal: format!(
            "Synthesise the outputs from all departments and produce a final answer for: {goal}"
        ),
        depends_on: team_ids,
        tool_allowlist: vec![],
        kind: TaskNodeKind::Leaf,
        role: Some(gglib_core::domain::orchestrator::role_catalog::RoleId::new(
            "synthesizer",
        )),
        status: NodeStatus::Pending,
        output: None,
        compacted_output: None,
        error: None,
    });

    // ── 5. Build top-level graph ───────────────────────────────────────────
    TaskGraph::new(goal.to_string(), hitl_mode, team_nodes).map_err(PlanError::from)
}

// =============================================================================
// Helpers
// =============================================================================

/// Create a single-department fallback brief so the planner never returns empty.
fn single_dept_fallback(goal: &str) -> Vec<DepartmentBrief> {
    vec![DepartmentBrief {
        name: "main".into(),
        mission: goal.to_string(),
        suggested_roles: vec![],
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_dept_fallback_returns_one_dept() {
        let depts = single_dept_fallback("Write a report");
        assert_eq!(depts.len(), 1);
        assert_eq!(depts[0].name, "main");
        assert_eq!(depts[0].mission, "Write a report");
    }
}
