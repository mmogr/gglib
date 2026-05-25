//! Director agent: decompose a goal into a validated [`TaskGraph`].
//!
//! The [`plan`] function runs a structured-output loop via
//! [`crate::structured_output::get_structured`], converts the LLM response
//! into a [`TaskGraph`], and validates it before returning.
//!
//! # Retry loop
//!
//! When validation fails the director appends an error-feedback message to the
//! conversation history and retries, up to `max_replans + 1` total attempts.
//! This mirrors how [`crate::structured_output::get_structured`] handles JSON
//! parse failures: the model sees its own broken output and typically
//! self-corrects quickly.
//!
//! # No execution
//!
//! The director only **plans**.  No worker agents are spawned here.  All
//! execution logic is Phase C+.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::mpsc;

use gglib_core::domain::agent::{AgentMessage, AssistantContent, ToolDefinition};
use gglib_core::domain::council::events::CouncilEvent;
use gglib_core::domain::council::role_catalog::{RoleCatalog, RoleId};
use gglib_core::domain::council::task_graph::{
    HitlMode, NodeId, NodeStatus, TaskGraph, TaskGraphError, TaskNode, TaskNodeKind,
};
use gglib_core::ports::LlmCompletionPort;

use gglib_core::ports::StructuredOutputError;

use crate::council::chief_of_staff::{DepartmentBrief, render_role_catalog};
use crate::council::prompts::{DIRECTOR_SYSTEM_PROMPT, director_plan_schema};
use crate::structured_output::get_structured;

// =============================================================================
// PlanError
// =============================================================================

/// Error variants produced by [`plan`].
#[derive(Debug, Error)]
pub enum PlanError {
    /// The structured-output adapter failed (stream error, parse error, etc.).
    #[error("structured output error: {0}")]
    StructuredOutput(#[from] StructuredOutputError),

    /// Task-graph validation failed after all replan attempts.
    #[error("plan validation failed after {max_replans} attempt(s): {last_error}")]
    ValidationFailed {
        /// Number of replan attempts that were allowed.
        max_replans: u32,
        /// The validation error from the final attempt.
        last_error: String,
    },
}

impl From<TaskGraphError> for PlanError {
    fn from(e: TaskGraphError) -> Self {
        Self::ValidationFailed {
            max_replans: 0,
            last_error: e.to_string(),
        }
    }
}

// =============================================================================
// Internal wire types (not part of public API)
// =============================================================================

/// Intermediate representation returned by the LLM before conversion to
/// [`TaskGraph`].
///
/// Uses a flat list of nodes instead of a `HashMap` so the JSON schema and
/// prompt examples stay simple and unambiguous for small models.
///
/// # Example (doc-test: schema round-trip, no LLM required)
///
/// ```rust
/// use gglib_agent::council::director::DirectorPlan;
/// use serde_json::json;
///
/// let raw = json!({
///     "goal": "example",
///     "nodes": [{
///         "id": "a",
///         "goal": "do a",
///         "depends_on": [],
///         "tool_allowlist": []
///     }]
/// });
/// let plan: DirectorPlan = serde_json::from_value(raw).unwrap();
/// assert_eq!(plan.nodes.len(), 1);
/// assert_eq!(plan.nodes[0].id, "a");
/// ```
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectorPlan {
    /// Mirror of [`TaskGraph::goal`].
    pub goal: String,
    /// All work units as a flat ordered list.
    pub nodes: Vec<DirectorNode>,
}

/// A single work unit within a [`DirectorPlan`].
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectorNode {
    /// Short, unique kebab-case identifier.
    pub id: String,
    /// One-sentence goal for the worker agent.
    pub goal: String,
    /// Ids of prerequisite nodes.
    pub depends_on: Vec<String>,
    /// Tool names the worker is permitted to use.
    pub tool_allowlist: Vec<String>,
}

// =============================================================================
// Constants
// =============================================================================

/// Maximum number of predecessors any single node may declare.
///
/// Prevents "fan-in" bottlenecks where one synthesis node must wait for an
/// unbounded number of parallel workers before it can start.
const MAX_IN_DEGREE: usize = 4;

// =============================================================================
// Public API
// =============================================================================

/// Decompose `goal` into a validated [`TaskGraph`] using the director LLM.
///
/// Runs a retry loop: each attempt calls the LLM with a JSON Schema constraint
/// via [`get_structured`], then validates the result against:
///
/// 1. Structural invariants (acyclicity, size, depth) — [`TaskGraph::validate_acyclic`].
/// 2. Tool allowlist (only when `tools` is non-empty) — [`TaskGraph::validate_tool_allowlist`].
/// 3. In-degree cap ([`MAX_IN_DEGREE`] predecessors per node).
///
/// On validation failure the error is appended to the conversation history and
/// the loop retries up to `max_replans + 1` total attempts.  If all attempts
/// fail, [`PlanError::ValidationFailed`] is returned.
///
/// # Parameters
///
/// - `goal` — High-level goal to decompose (or the original goal when called
///   from the hierarchical planner — each department's `mission` is prepended).
/// - `department` — Optional department brief from the Chief of Staff.  When
///   `Some`, the user message is prefixed with the department name and mission,
///   and `suggested_roles` are round-robin-assigned to leaf nodes.
/// - `catalog` — Role catalog used to validate and render role information.
/// - `tools` — Available tool catalog.  Pass `&[]` to skip allowlist
///   validation (useful when no executor is running).
/// - `llm` — LLM completion port.
/// - `hitl_mode` — Human-in-the-loop policy to embed in the returned graph.
/// - `max_replans` — Number of *extra* attempts after the first.  `0` means
///   try once with no retry.
/// - `tx` — Optional event channel; when `Some`, emits [`CouncilEvent`]s
///   for each replan attempt so callers can stream progress via SSE.
///
/// # Errors
///
/// Returns [`PlanError`] if the LLM stream fails or all validation attempts
/// are exhausted.
///
/// # Example (doc-test: conversion only, no LLM)
///
/// ```rust
/// use gglib_agent::council::director::{DirectorNode, DirectorPlan};
/// use gglib_core::domain::council::task_graph::{NodeId, TaskNode, NodeStatus};
///
/// let node = DirectorNode {
///     id: "research".into(),
///     goal: "Research the topic".into(),
///     depends_on: vec![],
///     tool_allowlist: vec![],
/// };
/// assert_eq!(node.id, "research");
///
/// let plan = DirectorPlan {
///     goal: "Write a doc".into(),
///     nodes: vec![node],
/// };
/// assert_eq!(plan.nodes.len(), 1);
/// ```
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn plan(
    goal: &str,
    department: Option<&DepartmentBrief>,
    catalog: &RoleCatalog,
    tools: &[ToolDefinition],
    llm: Arc<dyn LlmCompletionPort>,
    hitl_mode: HitlMode,
    max_replans: u32,
    tx: Option<mpsc::Sender<CouncilEvent>>,
) -> Result<TaskGraph, PlanError> {
    let tool_catalog = render_tool_catalog(tools);
    let role_catalog_text = render_role_catalog(catalog);
    #[allow(clippy::literal_string_with_formatting_args)]
    let system = DIRECTOR_SYSTEM_PROMPT
        .replace("{tool_catalog}", &tool_catalog)
        .replace("{role_catalog}", &role_catalog_text);

    // Build the user goal: if we have a department brief, prefix with its mission.
    let user_goal = department.map_or_else(
        || goal.to_owned(),
        |dept| {
            format!(
                "Department: {}\nMission: {}\n\nGoal: {}",
                dept.name, dept.mission, goal
            )
        },
    );

    let mut messages: Vec<AgentMessage> = vec![
        AgentMessage::System { content: system },
        AgentMessage::User { content: user_goal },
    ];

    let schema = director_plan_schema();
    let total_attempts = max_replans + 1;
    let mut last_error = String::new();

    for attempt in 0..total_attempts {
        if attempt > 0 {
            tracing::debug!(
                attempt,
                goal,
                "director: retrying plan after validation failure"
            );
            if let Some(ref tx) = tx {
                let _ = tx
                    .send(CouncilEvent::ReplanAttempt {
                        attempt,
                        reason: last_error.clone(),
                    })
                    .await;
            }
        }

        let director_plan = match get_structured::<DirectorPlan>(
            &llm,
            messages.clone(),
            schema.clone(),
            // single parse retry within get_structured; outer loop handles
            // validation retries
            1,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => return Err(PlanError::StructuredOutput(e)),
        };

        // Convert flat nodes to TaskNode vec.
        // If a department brief supplies roles, round-robin-assign them to nodes.
        let suggested_roles: &[RoleId] = department.map_or(&[], |d| d.suggested_roles.as_slice());

        let task_nodes: Vec<TaskNode> = director_plan
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let role = if suggested_roles.is_empty() {
                    None
                } else {
                    Some(suggested_roles[i % suggested_roles.len()].clone())
                };
                TaskNode {
                    id: NodeId(n.id.clone()),
                    goal: n.goal.clone(),
                    depends_on: n.depends_on.iter().map(|d| NodeId(d.clone())).collect(),
                    tool_allowlist: n.tool_allowlist.clone(),
                    kind: TaskNodeKind::Leaf,
                    role,
                    status: NodeStatus::Pending,
                    output: None,
                    compacted_output: None,
                    error: None,
                }
            })
            .collect();

        // Build the graph (validates acyclicity, unknown deps, size, depth).
        let graph = match TaskGraph::new(director_plan.goal.clone(), hitl_mode.clone(), task_nodes)
        {
            Ok(g) => g,
            Err(e) => {
                let err = e.to_string();
                last_error.clone_from(&err);
                push_error_feedback(&mut messages, &director_plan, &err);
                continue;
            }
        };

        // Tool allowlist check (only when a catalog is provided).
        if !tools.is_empty() {
            if let Err(e) = graph.validate_tool_allowlist(tools) {
                let err = e.to_string();
                last_error.clone_from(&err);
                push_error_feedback(&mut messages, &director_plan, &err);
                continue;
            }
        }

        // In-degree cap.
        if let Some(err) = check_in_degree(&graph) {
            last_error.clone_from(&err);
            push_error_feedback(&mut messages, &director_plan, &err);
            continue;
        }

        // All checks passed.
        tracing::debug!(
            goal,
            nodes = graph.nodes.len(),
            "director: plan accepted on attempt {attempt}"
        );
        return Ok(graph);
    }

    Err(PlanError::ValidationFailed {
        max_replans,
        last_error,
    })
}

// =============================================================================
// Helpers
// =============================================================================

/// Render tool catalog as one `"- name: description"` line per tool.
///
/// Only name + first-line description is included — NOT full JSON schemas —
/// to keep the prompt compact for small models.
fn render_tool_catalog(tools: &[ToolDefinition]) -> String {
    if tools.is_empty() {
        return "(no tools available — use empty tool_allowlist for all nodes)".to_owned();
    }
    tools
        .iter()
        .map(|t| {
            let desc = t.description.as_deref().unwrap_or("(no description)");
            // Take only the first line of multi-line descriptions.
            let first_line = desc.lines().next().unwrap_or(desc);
            format!("- {}: {}", t.name, first_line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check that no node has more than [`MAX_IN_DEGREE`] predecessors.
fn check_in_degree(graph: &TaskGraph) -> Option<String> {
    // Build the in-degree map: count how many nodes depend on each node.
    let mut counts: HashMap<&NodeId, usize> = HashMap::new();
    for node in graph.nodes.values() {
        for dep in &node.depends_on {
            *counts.entry(dep).or_insert(0) += 1;
        }
    }
    // Also check direct in-degree (depends_on.len()) per node.
    for (id, node) in &graph.nodes {
        if node.depends_on.len() > MAX_IN_DEGREE {
            return Some(format!(
                "node '{}' lists {} predecessors; maximum in-degree is {}",
                id,
                node.depends_on.len(),
                MAX_IN_DEGREE
            ));
        }
    }
    None
}

/// Append error-feedback messages so the model can self-correct on the next
/// attempt.
fn push_error_feedback(messages: &mut Vec<AgentMessage>, plan: &DirectorPlan, error: &str) {
    // Echo the model's broken plan back as an assistant turn.
    let plan_json = serde_json::to_string(plan).unwrap_or_default();
    messages.push(AgentMessage::Assistant {
        content: AssistantContent {
            text: Some(plan_json),
            tool_calls: vec![],
        },
    });
    messages.push(AgentMessage::User {
        content: format!(
            "Your plan was invalid: {error}\n\n\
             Please output a corrected JSON plan that satisfies all constraints."
        ),
    });
}
