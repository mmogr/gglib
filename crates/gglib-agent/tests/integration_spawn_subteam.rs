//! Integration test: `spawn_subteam` dynamic sub-team spawning (Phase I).
//!
//! Tests:
//! 1. A worker that calls `spawn_subteam` triggers a [`SubteamSpawned`] event
//!    and the child sub-team executes; the run completes successfully.
//! 2. With [`HitlMode::None`] the spawn is auto-approved (no
//!    [`AwaitingApproval`] event for the spawn).
//! 3. With a HITL registry and [`HitlMode::ApproveEachNode`], an
//!    [`AwaitingApproval`] event with `kind.spawn_subteam` is emitted before
//!    the child runs.
//!
//! # LLM call budget (auto-approve path)
//!
//! Main worker: 2 calls (initial + continuation after spawn tool reply)
//! Main worker compaction: 1 call
//! Spawn planning — CoS: 1 call
//! Spawn planning — Director (1 dept, 2 leaves): 1 call
//! Spawned leaf workers (2): 2 calls
//! Spawned leaf compactions (2): 2 calls
//! Top-level synthesis: 1 call
//! Total: 10 calls

#![allow(unused_crate_dependencies)]

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::mock_llm::{MockLlmPort, MockLlmResponse};
use gglib_agent::orchestrator::{OrchestratorConfig, execute};
use gglib_core::domain::orchestrator::events::{ApprovalKind, OrchestratorEvent};
use gglib_core::domain::orchestrator::task_graph::{
    HitlMode, NodeId, NodeStatus, TaskGraph, TaskNode, TaskNodeKind,
};
use gglib_core::ports::{ApprovalDecision, EmptyToolExecutor, OrchestratorApprovalRegistryPort};
use tokio::sync::{mpsc, oneshot};

// =============================================================================
// Simple approval registry for tests
// =============================================================================

/// A trivial in-process registry used in tests.
struct TestApprovalRegistry {
    senders: tokio::sync::Mutex<HashMap<String, oneshot::Sender<ApprovalDecision>>>,
}

impl TestApprovalRegistry {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            senders: tokio::sync::Mutex::new(HashMap::new()),
        })
    }
}

impl OrchestratorApprovalRegistryPort for TestApprovalRegistry {
    fn register(&self, approval_id: String, sender: oneshot::Sender<ApprovalDecision>) {
        let mut guard = self.senders.try_lock().unwrap();
        guard.insert(approval_id, sender);
    }

    fn resolve(&self, approval_id: &str, decision: ApprovalDecision) -> bool {
        let mut guard = self.senders.try_lock().unwrap();
        if let Some(tx) = guard.remove(approval_id) {
            tx.send(decision).is_ok()
        } else {
            false
        }
    }

    fn is_pending(&self, approval_id: &str) -> bool {
        self.senders.try_lock().unwrap().contains_key(approval_id)
    }
}

// =============================================================================
// Scripted LLM helpers
// =============================================================================

/// Single-node graph that simply does one leaf task.
fn single_leaf_graph() -> TaskGraph {
    let node = TaskNode {
        id: NodeId("worker-1".into()),
        goal: "Research the topic and spawn a writing team if needed.".into(),
        depends_on: vec![],
        tool_allowlist: vec!["spawn_subteam".into()],
        kind: TaskNodeKind::Leaf,
        role: None,
        status: NodeStatus::Pending,
        output: None,
        compacted_output: None,
        error: None,
    };
    TaskGraph::new(
        "Research and spawn writing.".into(),
        HitlMode::None,
        vec![node],
    )
    .expect("single-node graph must be valid")
}

// =============================================================================
// Tests
// =============================================================================

/// Happy path: worker calls spawn_subteam → auto-approved (HitlMode::None) →
/// child subgraph runs → SubteamSpawned event emitted → run completes.
#[tokio::test]
async fn spawn_subteam_auto_approve() {
    // LLM call budget:
    // [0]  initial worker turn → tool call for spawn_subteam
    // [1]  continuation after spawn tool returns
    // [2]  main worker compaction
    // [3]  CoS for child planning (1 dept)
    // [4]  Director for child planning (2 leaf nodes: write-a, write-b)
    // [5]  write-a worker
    // [6]  write-a compaction
    // [7]  write-b worker
    // [8]  write-b compaction
    // [9]  synthesizer leaf worker (appended by planner)
    // [10] synthesizer leaf compaction
    // [11] top-level synthesis
    let child_cos = serde_json::json!({
        "departments": [{
            "name": "writing",
            "mission": "Write the deliverable.",
            "suggested_roles": ["writer"]
        }]
    })
    .to_string();

    let child_director = serde_json::json!({
        "goal": "Write the deliverable",
        "nodes": [
            {
                "id": "write-a",
                "goal": "Draft the document.",
                "depends_on": [],
                "tool_allowlist": []
            },
            {
                "id": "write-b",
                "goal": "Proofread the document.",
                "depends_on": ["write-a"],
                "tool_allowlist": []
            }
        ]
    })
    .to_string();

    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "tc-spawn-1",
                "spawn_subteam",
                serde_json::json!({
                    "goal": "Write a polished document based on the research.",
                    "suggested_roles": ["writer", "proofreader"]
                }),
            ))
            .push(MockLlmResponse::text(
                "Research complete, spawning writing team.",
            ))
            .push(MockLlmResponse::text("Done.")) // [2] main worker compaction
            .push(MockLlmResponse::text(child_cos)) // [3] CoS
            .push(MockLlmResponse::text(child_director)) // [4] Director
            .push_many((0..7).map(|_| MockLlmResponse::text("Done."))), // [5..11]
    );

    let tool_executor = Arc::new(EmptyToolExecutor);

    let (tx, mut rx) = mpsc::channel::<OrchestratorEvent>(4096);

    let config = OrchestratorConfig {
        hitl_mode: HitlMode::None,
        max_replans: 0,
        graph_override: Some(single_leaf_graph()),
        ..Default::default()
    };

    let run_handle = tokio::spawn(async move {
        execute(
            "Research and write a comprehensive analysis.",
            &[],
            llm,
            tool_executor,
            config,
            tx,
        )
        .await
    });

    let mut events: Vec<OrchestratorEvent> = Vec::new();
    while let Some(ev) = rx.recv().await {
        events.push(ev);
    }

    let result = run_handle.await.expect("task did not panic");
    assert!(result.is_ok(), "execute should succeed: {result:?}");

    // ── Verify SubteamSpawned was emitted ─────────────────────────────────────
    let subteam_spawned = events.iter().any(|e| {
        matches!(e, OrchestratorEvent::SubteamSpawned { parent_node_id, .. }
            if parent_node_id == "worker-1")
    });
    assert!(
        subteam_spawned,
        "SubteamSpawned event should be emitted for worker-1"
    );

    // ── Verify no AwaitingApproval with spawn_subteam kind ────────────────────
    let spawn_approval = events.iter().any(|e| {
        matches!(
            e,
            OrchestratorEvent::AwaitingApproval {
                kind: ApprovalKind::SpawnSubteam { .. },
                ..
            }
        )
    });
    assert!(
        !spawn_approval,
        "HitlMode::None should not emit AwaitingApproval for spawn"
    );

    // ── Verify run completed ──────────────────────────────────────────────────
    assert!(
        matches!(
            events.last(),
            Some(OrchestratorEvent::OrchestratorComplete { .. })
        ),
        "last event should be OrchestratorComplete"
    );
}

/// HITL path: with ApproveEachNode, spawn requires explicit approval.
/// The test auto-approves via the registry to keep it non-blocking.
#[tokio::test]
async fn spawn_subteam_hitl_requires_approval() {
    let child_cos = serde_json::json!({
        "departments": [{
            "name": "writing",
            "mission": "Write the deliverable.",
            "suggested_roles": ["writer"]
        }]
    })
    .to_string();

    let child_director = serde_json::json!({
        "goal": "Write the deliverable",
        "nodes": [
            {
                "id": "write-a",
                "goal": "Draft the document.",
                "depends_on": [],
                "tool_allowlist": []
            },
            {
                "id": "write-b",
                "goal": "Proofread the document.",
                "depends_on": ["write-a"],
                "tool_allowlist": []
            }
        ]
    })
    .to_string();

    let llm = Arc::new(
        MockLlmPort::new()
            .push(MockLlmResponse::tool_call(
                "tc-spawn-2",
                "spawn_subteam",
                serde_json::json!({
                    "goal": "Write a polished document.",
                    "suggested_roles": ["writer"]
                }),
            ))
            .push(MockLlmResponse::text("Research complete."))
            .push(MockLlmResponse::text("Done.")) // main worker compaction
            .push(MockLlmResponse::text(child_cos)) // CoS
            .push(MockLlmResponse::text(child_director)) // Director
            .push_many((0..7).map(|_| MockLlmResponse::text("Done."))),
    );

    let tool_executor = Arc::new(EmptyToolExecutor);
    let registry = TestApprovalRegistry::new();
    let registry_for_resolver = Arc::clone(&registry);

    let (tx, mut rx) = mpsc::channel::<OrchestratorEvent>(4096);

    let config = OrchestratorConfig {
        hitl_mode: HitlMode::ApproveEachNode,
        max_replans: 0,
        graph_override: Some(single_leaf_graph()),
        approval_registry: Some(registry),
        ..Default::default()
    };

    // Spawn the executor.
    let run_handle = tokio::spawn(async move {
        execute(
            "Research and write a comprehensive analysis.",
            &[],
            llm,
            tool_executor,
            config,
            tx,
        )
        .await
    });

    // Consume events; whenever we see AwaitingApproval, approve it immediately.
    let mut events: Vec<OrchestratorEvent> = Vec::new();
    while let Some(ev) = rx.recv().await {
        if let OrchestratorEvent::AwaitingApproval { approval_id, .. } = &ev {
            let id = approval_id.clone();
            let reg = Arc::clone(&registry_for_resolver);
            tokio::spawn(async move {
                reg.resolve(&id, ApprovalDecision::Approve);
            });
        }
        events.push(ev);
    }

    let result = run_handle.await.expect("task did not panic");
    assert!(
        result.is_ok(),
        "execute should succeed with approved spawn: {result:?}"
    );

    // ── Verify AwaitingApproval with SpawnSubteam kind was emitted ────────────
    let spawn_approval_event = events.iter().find(|e| {
        matches!(
            e,
            OrchestratorEvent::AwaitingApproval {
                kind: ApprovalKind::SpawnSubteam { node_id, .. },
                ..
            } if node_id == "worker-1"
        )
    });
    assert!(
        spawn_approval_event.is_some(),
        "ApproveEachNode should emit AwaitingApproval(SpawnSubteam) for worker-1"
    );

    // ── Verify SubteamSpawned was emitted ─────────────────────────────────────
    let subteam_spawned = events.iter().any(|e| {
        matches!(e, OrchestratorEvent::SubteamSpawned { parent_node_id, .. }
            if parent_node_id == "worker-1")
    });
    assert!(
        subteam_spawned,
        "SubteamSpawned should be emitted after approval"
    );

    // ── Verify run completed ──────────────────────────────────────────────────
    assert!(
        matches!(
            events.last(),
            Some(OrchestratorEvent::OrchestratorComplete { .. })
        ),
        "last event should be OrchestratorComplete"
    );
}
