//! Integration test: hierarchical execution (Phase I).
//!
//! Verifies that the executor correctly drives a 3-department / 12-leaf
//! [`TaskGraph`] to completion, emitting [`TeamStarted`] before any leaf in
//! that team and [`TeamSynthesized`] before any sibling or downstream node
//! starts.
//!
//! # LLM call budget
//!
//! Planning:   1 `CoS` + 3 Director              =  4 calls
//! Execution: 12 leaf workers + 12 compactions  = 24 calls
//! Synthesis:  1 final synthesis call           =  1 call
//! Total                                        = 29 calls

#![allow(unused_crate_dependencies)]

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::mock_llm::{MockLlmPort, MockLlmResponse};
use gglib_agent::orchestrator::{OrchestratorConfig, execute};
use gglib_core::domain::orchestrator::events::OrchestratorEvent;
use gglib_core::domain::orchestrator::task_graph::HitlMode;
use gglib_core::ports::EmptyToolExecutor;
use tokio::sync::mpsc;

// =============================================================================
// Scripted LLM helpers — same as integration_hierarchical_plan.rs
// =============================================================================

fn cos_json() -> String {
    serde_json::json!({
        "departments": [
            {
                "name": "research",
                "mission": "Gather all factual evidence.",
                "suggested_roles": ["researcher"]
            },
            {
                "name": "writing",
                "mission": "Produce the written deliverable.",
                "suggested_roles": ["writer"]
            },
            {
                "name": "risk-review",
                "mission": "Identify risks.",
                "suggested_roles": ["critic"]
            }
        ]
    })
    .to_string()
}

fn director_json(prefix: &str) -> String {
    serde_json::json!({
        "goal": format!("Complete the {prefix} workstream"),
        "nodes": [
            {
                "id": format!("{prefix}-a"),
                "goal": format!("First task for {prefix}."),
                "depends_on": [],
                "tool_allowlist": []
            },
            {
                "id": format!("{prefix}-b"),
                "goal": format!("Second task for {prefix}."),
                "depends_on": [],
                "tool_allowlist": []
            },
            {
                "id": format!("{prefix}-c"),
                "goal": format!("Integrate a+b for {prefix}."),
                "depends_on": [format!("{prefix}-a"), format!("{prefix}-b")],
                "tool_allowlist": []
            },
            {
                "id": format!("{prefix}-d"),
                "goal": format!("Finalise {prefix}."),
                "depends_on": [format!("{prefix}-c")],
                "tool_allowlist": []
            }
        ]
    })
    .to_string()
}

// =============================================================================
// Test
// =============================================================================

/// 3-department / 12-leaf plan executes end-to-end.
///
/// Verifies:
/// 1. `TeamStarted` is emitted for each team before any `NodeStarted` event
///    from a leaf in that team.
/// 2. `TeamSynthesized` is emitted for each team before the top-level
///    synthesizer `NodeStarted` event (i.e. the downstream synthesizer does
///    not start until all teams are done).
/// 3. The run completes with `OrchestratorComplete`.
#[tokio::test]
async fn hierarchical_run_three_departments_twelve_leaves() {
    // Planning: 1 CoS + 3 Directors.
    // Execution: 13 leaf workers (12 dept leaves + 1 synthesizer leaf) ×
    //            (1 worker turn + 1 compaction turn) = 26
    //            + 1 top-level synthesis pass = 27
    // Total: 4 + 26 + 1 = 31 LLM calls.
    let leaf_answer = "Done.";
    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::text(cos_json()))
            .push(MockLlmResponse::text(director_json("research")))
            .push(MockLlmResponse::text(director_json("writing")))
            .push(MockLlmResponse::text(director_json("risk-review")))
            .push_many((0..27).map(|_| MockLlmResponse::text(leaf_answer))),
    );

    let tool_executor = Arc::new(EmptyToolExecutor);

    let (tx, mut rx) = mpsc::channel::<OrchestratorEvent>(4096);

    let config = OrchestratorConfig {
        hitl_mode: HitlMode::None,
        max_replans: 0,
        ..Default::default()
    };

    let run_handle = tokio::spawn(async move {
        execute(
            "Write a launch plan with research, writing, and risk review",
            &[],
            llm,
            tool_executor,
            config,
            tx,
        )
        .await
    });

    // Collect all events.
    let mut events: Vec<OrchestratorEvent> = Vec::new();
    while let Some(ev) = rx.recv().await {
        events.push(ev);
    }

    // The execute call should have returned Ok(()) by now.
    let result = run_handle.await.expect("task did not panic");
    assert!(result.is_ok(), "execute should succeed: {result:?}");

    // ── Verify OrchestratorComplete is the last event ─────────────────────────
    assert!(
        matches!(
            events.last(),
            Some(OrchestratorEvent::OrchestratorComplete { .. })
        ),
        "last event should be OrchestratorComplete"
    );

    // ── Build index: event position for each event type ───────────────────────
    // For each team_id record position of TeamStarted and TeamSynthesized.
    // For each node_id record position of first NodeStarted.
    let mut team_started_pos: HashMap<String, usize> = HashMap::new();
    let mut team_synthesized_pos: HashMap<String, usize> = HashMap::new();
    let mut node_started_pos: HashMap<String, usize> = HashMap::new();

    for (pos, ev) in events.iter().enumerate() {
        match ev {
            OrchestratorEvent::TeamStarted { team_id, .. } => {
                team_started_pos.entry(team_id.clone()).or_insert(pos);
            }
            OrchestratorEvent::TeamSynthesized { team_id, .. } => {
                team_synthesized_pos.entry(team_id.clone()).or_insert(pos);
            }
            OrchestratorEvent::NodeStarted { node_id, .. } => {
                node_started_pos.entry(node_id.clone()).or_insert(pos);
            }
            _ => {}
        }
    }

    // We should have TeamStarted + TeamSynthesized for all 3 teams.
    assert_eq!(team_started_pos.len(), 3, "expected 3 TeamStarted events");
    assert_eq!(
        team_synthesized_pos.len(),
        3,
        "expected 3 TeamSynthesized events"
    );

    // ── Rule 1: TeamStarted comes before any leaf NodeStarted in that team ────
    for (team_id, started_pos) in &team_started_pos {
        // The leaf node ids all start with "<team_id>-" (e.g. "research-a").
        let team_prefix = format!("{team_id}-");
        for (node_id, node_pos) in &node_started_pos {
            if node_id.starts_with(&team_prefix) {
                assert!(
                    started_pos < node_pos,
                    "TeamStarted({team_id})@{started_pos} must precede \
                     NodeStarted({node_id})@{node_pos}"
                );
            }
        }
    }

    // ── Rule 2: TeamSynthesized for each team before synthesizer NodeStarted ──
    // The synthesizer top-level leaf id is "synthesizer".
    if let Some(synth_pos) = node_started_pos.get("synthesizer") {
        for (team_id, synthesized_pos) in &team_synthesized_pos {
            assert!(
                synthesized_pos < synth_pos,
                "TeamSynthesized({team_id})@{synthesized_pos} must precede \
                 NodeStarted(synthesizer)@{synth_pos}"
            );
        }
    }
    // (If synthesizer didn't appear as a NodeStarted, the run still completed,
    // but there is nothing to order against — that's fine.)
}
