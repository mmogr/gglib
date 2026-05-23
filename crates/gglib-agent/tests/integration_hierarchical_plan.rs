//! Integration test: hierarchical planner (Phase H).
//!
//! Tests that [`planner::plan`] correctly assembles a nested [`TaskGraph`]
//! with Team nodes at the top level, given a scripted mock LLM.
//!
//! The mock LLM is configured to return:
//! 1. A Chief of Staff response with exactly 3 departments.
//! 2. Three Director responses, each with 4 leaf nodes.
//!
//! Expected resulting structure:
//! - 3 × `Team` nodes (one per department), each wrapping a 4-node subgraph.
//! - 1 × `synthesizer` Leaf node depending on all 3 Team nodes.
//! - Total node count: 3 (teams) + 1 (synthesizer) = 4 top-level nodes.
//! - `total_node_count()`: 3 × 4 (dept leaves) + 4 (top level) = 16.

#![allow(unused_crate_dependencies)]

mod common;

use std::sync::Arc;

use common::mock_llm::{MockLlmPort, MockLlmResponse};
use gglib_agent::orchestrator::planner;
use gglib_core::domain::orchestrator::task_graph::{HitlMode, TaskNodeKind};

// =============================================================================
// Helpers
// =============================================================================

/// Returns the scripted Chief-of-Staff JSON response for 3 departments.
fn cos_json() -> String {
    serde_json::json!({
        "departments": [
            {
                "name": "research",
                "mission": "Gather all factual evidence about the topic.",
                "suggested_roles": ["researcher", "fact-checker"]
            },
            {
                "name": "writing",
                "mission": "Produce and polish the written deliverable.",
                "suggested_roles": ["writer", "editor"]
            },
            {
                "name": "risk-review",
                "mission": "Identify risks and propose mitigations.",
                "suggested_roles": ["red-team", "critic"]
            }
        ]
    })
    .to_string()
}

/// Returns a scripted `DirectorPlan` JSON response for a department
/// with exactly 4 leaf nodes (2 parallel roots + 1 middle + 1 synthesizer).
fn director_json(prefix: &str) -> String {
    serde_json::json!({
        "goal": format!("Complete the {prefix} workstream"),
        "nodes": [
            {
                "id": format!("{prefix}-a"),
                "goal": format!("First parallel task for {prefix} department workstream."),
                "depends_on": [],
                "tool_allowlist": []
            },
            {
                "id": format!("{prefix}-b"),
                "goal": format!("Second parallel task for {prefix} department workstream."),
                "depends_on": [],
                "tool_allowlist": []
            },
            {
                "id": format!("{prefix}-c"),
                "goal": format!("Integrate outputs from tasks a and b for {prefix} department."),
                "depends_on": [format!("{prefix}-a"), format!("{prefix}-b")],
                "tool_allowlist": []
            },
            {
                "id": format!("{prefix}-d"),
                "goal": format!("Finalise and verify the {prefix} department deliverable."),
                "depends_on": [format!("{prefix}-c")],
                "tool_allowlist": []
            }
        ]
    })
    .to_string()
}

// =============================================================================
// Tests
// =============================================================================

/// Core structure test: 3 departments × 4 leaves each → 4 top-level nodes
/// (3 Team + 1 synthesizer), `total_node_count` = 16.
#[tokio::test]
async fn hierarchical_plan_three_departments_twelve_leaves() {
    // Queue: CoS response first, then 3 director responses (one per dept).
    // The director fan-out is parallel so responses are dequeued in
    // non-deterministic order — but since every dept response is valid for any
    // dept (4 nodes each), the test outcome is order-independent.
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::text(cos_json()))
            .push(MockLlmResponse::text(director_json("research")))
            .push(MockLlmResponse::text(director_json("writing")))
            .push(MockLlmResponse::text(director_json("risk-review"))),
    );

    let graph = planner::plan(
        "Write a launch plan with marketing, engineering, and risk review",
        &[],
        llm,
        HitlMode::None,
        0,
        None,
    )
    .await
    .expect("planner::plan should succeed");

    // Top-level: 3 Team nodes + 1 synthesizer.
    assert_eq!(
        graph.nodes.len(),
        4,
        "expected 4 top-level nodes (3 teams + synthesizer), got {}",
        graph.nodes.len()
    );

    // Exactly 3 Team nodes.
    let team_count = graph
        .nodes
        .values()
        .filter(|n| matches!(&n.kind, TaskNodeKind::Team { .. }))
        .count();
    assert_eq!(team_count, 3, "expected 3 Team nodes, got {team_count}");

    // Synthesizer leaf.
    let synth = graph
        .nodes
        .get(&gglib_core::domain::orchestrator::task_graph::NodeId(
            "synthesizer".into(),
        ))
        .expect("synthesizer node must exist");
    assert!(
        matches!(synth.kind, TaskNodeKind::Leaf),
        "synthesizer must be a Leaf node"
    );
    assert_eq!(
        synth.depends_on.len(),
        3,
        "synthesizer must depend on all 3 team nodes"
    );
    // Synthesizer should have the synthesizer role.
    assert_eq!(
        synth
            .role
            .as_ref()
            .map(gglib_core::domain::orchestrator::RoleId::as_str),
        Some("synthesizer"),
        "synthesizer node must have role=synthesizer"
    );

    // Each Team node wraps a 4-node subgraph.
    for (id, node) in &graph.nodes {
        if let TaskNodeKind::Team { subgraph } = &node.kind {
            assert_eq!(
                subgraph.nodes.len(),
                4,
                "Team node '{id}' subgraph should have 4 leaves, got {}",
                subgraph.nodes.len()
            );
            // All nodes in the subgraph must be Leaf nodes (no nested teams in this test).
            for (sub_id, sub_node) in &subgraph.nodes {
                assert!(
                    matches!(sub_node.kind, TaskNodeKind::Leaf),
                    "sub-node '{sub_id}' inside team '{id}' must be a Leaf"
                );
            }
        }
    }

    // total_node_count: 4 top-level + 3*4 subgraph = 16.
    assert_eq!(
        graph.total_node_count(),
        16,
        "total_node_count should be 16 (4 top + 3×4 subgraph)"
    );
}

/// Single-department goal produces 1 Team + 1 synthesizer (2 top-level nodes).
#[tokio::test]
async fn hierarchical_plan_single_department() {
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::text(
                serde_json::json!({
                    "departments": [{
                        "name": "summarisation",
                        "mission": "Read and summarise the article.",
                        "suggested_roles": ["researcher", "writer"]
                    }]
                })
                .to_string(),
            ))
            .push(MockLlmResponse::text(director_json("summarisation"))),
    );

    let graph = planner::plan(
        "Summarise this article about climate change",
        &[],
        llm,
        HitlMode::None,
        0,
        None,
    )
    .await
    .expect("single-department plan should succeed");

    assert_eq!(
        graph.nodes.len(),
        2,
        "single-department: expected 2 top-level nodes (1 team + synthesizer)"
    );

    let team_count = graph
        .nodes
        .values()
        .filter(|n| matches!(&n.kind, TaskNodeKind::Team { .. }))
        .count();
    assert_eq!(team_count, 1);

    // total_node_count: 2 top + 4 subgraph = 6.
    assert_eq!(graph.total_node_count(), 6);
}

/// `CoS` returning an empty list triggers fallback → 1 team + 1 synthesizer.
#[tokio::test]
async fn hierarchical_plan_empty_cos_falls_back() {
    let llm = Arc::new(
        MockLlmPort::new()
            // Empty departments list — triggers fallback.
            .push(MockLlmResponse::text(
                serde_json::json!({ "departments": [] }).to_string(),
            ))
            // Director plan for the fallback single "main" department.
            .push(MockLlmResponse::text(director_json("main"))),
    );

    let graph = planner::plan("Fallback goal", &[], llm, HitlMode::None, 0, None)
        .await
        .expect("fallback plan should succeed");

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(
        graph
            .nodes
            .values()
            .filter(|n| matches!(&n.kind, TaskNodeKind::Team { .. }))
            .count(),
        1
    );
}

/// Roles are round-robin-assigned from the department brief to leaf nodes.
#[tokio::test]
async fn department_roles_assigned_to_leaves() {
    // Single department with 2 suggested roles and a 4-node director response.
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::text(
                serde_json::json!({
                    "departments": [{
                        "name": "writing",
                        "mission": "Write and polish the deliverable.",
                        "suggested_roles": ["writer", "editor"]
                    }]
                })
                .to_string(),
            ))
            .push(MockLlmResponse::text(director_json("writing"))),
    );

    let graph = planner::plan("Write a blog post", &[], llm, HitlMode::None, 0, None)
        .await
        .expect("plan should succeed");

    // Find the team node.
    let team_node = graph
        .nodes
        .values()
        .find(|n| matches!(&n.kind, TaskNodeKind::Team { .. }))
        .expect("must have a team node");

    if let TaskNodeKind::Team { subgraph } = &team_node.kind {
        // Collect roles from all leaf nodes — should be a mix of writer/editor.
        let roles: Vec<_> = subgraph
            .nodes
            .values()
            .filter_map(|n| n.role.as_ref().map(|r| r.as_str().to_owned()))
            .collect();

        // All 4 nodes should have a role assigned (round-robin from 2 roles).
        assert_eq!(
            roles.len(),
            4,
            "all 4 leaf nodes should have roles assigned"
        );
        // Both writer and editor should appear.
        assert!(
            roles.iter().any(|r| r == "writer"),
            "writer role should appear"
        );
        assert!(
            roles.iter().any(|r| r == "editor"),
            "editor role should appear"
        );
    } else {
        panic!("expected Team node");
    }
}
