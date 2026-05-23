//! Integration tests for the Phase D HITL approval gates.
//!
//! Uses a stub [`LlmCompletionPort`] (no real server required) combined with
//! the real [`OrchestratorApprovalRegistry`] and in-memory
//! [`SqliteOrchestratorRepository`] to exercise the gate logic end-to-end.
//!
//! # What is tested
//!
//! 1. **`hitl_approve_plan_continues_execution`** — with `HitlMode::ApprovePlan`
//!    the executor pauses at the plan gate, emits `AwaitingApproval`, and
//!    continues to `OrchestratorComplete` after the registry is resolved with
//!    `Approve`.
//!
//! 2. **`hitl_reject_plan_aborts_run`** — rejecting the plan emits
//!    `PlanRejected` and `execute()` returns `ExecuteError::PlanRejected`.
//!
//! 3. **`interrupted_runs_marked_on_startup`** — inserting a `Running` run
//!    into the DB and calling `mark_interrupted_runs()` transitions it to
//!    `Interrupted`.
//!
//! 4. **`hitl_none_mode_auto_approves`** — `HitlMode::None` (default) skips
//!    the gate entirely, emits `PlanApproved` without interaction, and the run
//!    completes successfully.

#![allow(unused_crate_dependencies)]

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use async_trait::async_trait;
use futures_util::stream;
use tokio::sync::mpsc;

use gglib_agent::orchestrator::{OrchestratorConfig, execute};
use gglib_app_services::OrchestratorApprovalRegistry;
use gglib_core::domain::orchestrator::events::OrchestratorEvent;
use gglib_core::domain::orchestrator::task_graph::HitlMode;
use gglib_core::ports::{
    ApprovalDecision, EmptyToolExecutor, LlmCompletionPort, OrchestratorApprovalRegistryPort,
    OrchestratorRepositoryPort, ResponseFormat, ToolExecutorPort,
};
use gglib_core::{AgentMessage, LlmStreamEvent, ToolDefinition};
use gglib_db::repositories::SqliteOrchestratorRepository;
use gglib_db::setup::setup_test_database;

// =============================================================================
// Stub LLM — identical pattern to orchestrator_execute.rs
// =============================================================================

type ResponseQueue = Arc<Mutex<VecDeque<Vec<LlmStreamEvent>>>>;

#[derive(Clone)]
struct StubLlm {
    queue: ResponseQueue,
}

impl StubLlm {
    fn new(responses: Vec<Vec<LlmStreamEvent>>) -> Self {
        Self {
            queue: Arc::new(Mutex::new(responses.into())),
        }
    }

    fn text_then_done(text: &str) -> Vec<LlmStreamEvent> {
        vec![
            LlmStreamEvent::TextDelta {
                content: text.to_owned(),
            },
            LlmStreamEvent::Done {
                finish_reason: "stop".into(),
            },
        ]
    }
}

#[async_trait]
impl LlmCompletionPort for StubLlm {
    async fn chat_stream(
        &self,
        _messages: &[AgentMessage],
        _tools: &[ToolDefinition],
        _response_format: Option<&ResponseFormat>,
    ) -> anyhow::Result<
        Pin<Box<dyn futures_core::Stream<Item = anyhow::Result<LlmStreamEvent>> + Send>>,
    > {
        let mut queue = self.queue.lock().unwrap();
        #[allow(clippy::option_if_let_else)]
        if let Some(events) = queue.pop_front() {
            let items: Vec<anyhow::Result<LlmStreamEvent>> = events.into_iter().map(Ok).collect();
            Ok(Box::pin(stream::iter(items)))
        } else {
            Err(anyhow!("StubLlm: no more canned responses"))
        }
    }
}

// =============================================================================
// Helpers
// =============================================================================

fn one_node_plan_json() -> String {
    serde_json::json!({
        "goal": "Answer a simple question",
        "nodes": [
            {
                "id": "answer",
                "goal": "Provide a concise answer",
                "depends_on": [],
                "tool_allowlist": []
            }
        ]
    })
    .to_string()
}

/// Chief-of-Staff response for a single department — consumed by the
/// hierarchical planner before the Director call.
fn cos_single_dept_json() -> String {
    serde_json::json!({
        "departments": [
            {
                "name": "main",
                "mission": "Complete the task.",
                "suggested_roles": []
            }
        ]
    })
    .to_string()
}

async fn collect_events(mut rx: mpsc::Receiver<OrchestratorEvent>) -> Vec<OrchestratorEvent> {
    let mut events = Vec::new();
    while let Some(e) = rx.recv().await {
        events.push(e);
    }
    events
}

async fn make_repo() -> Arc<SqliteOrchestratorRepository> {
    let pool = setup_test_database().await.unwrap();
    Arc::new(SqliteOrchestratorRepository::new(pool))
}

// =============================================================================
// Tests
// =============================================================================

/// Gate: `ApprovePlan` — approve → run completes with `OrchestratorComplete`.
#[tokio::test]
async fn hitl_approve_plan_continues_execution() {
    // LLM sequence: CoS → director → [gate] → worker → compaction →
    //   synthesizer worker → synthesizer compaction → final synthesis
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&one_node_plan_json()),
        StubLlm::text_then_done("The answer is 42."),
        StubLlm::text_then_done("Worker answered 42."),
        StubLlm::text_then_done("Synthesizer output."),
        StubLlm::text_then_done("Synthesizer compacted."),
        StubLlm::text_then_done("42."),
    ]);

    let registry = Arc::new(OrchestratorApprovalRegistry::new());
    let repo = make_repo().await;
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);

    let (tx, mut rx) = mpsc::channel(1024);

    // Spawn the executor in a task so we can interact with the gate.
    let registry_clone = registry.clone();
    let tx_clone = tx.clone();
    let exec_handle = tokio::spawn(async move {
        execute(
            "Answer a simple question",
            &[],
            Arc::new(llm),
            tool_executor,
            OrchestratorConfig {
                hitl_mode: HitlMode::ApprovePlan,
                approval_registry: Some(
                    registry_clone as Arc<dyn OrchestratorApprovalRegistryPort>,
                ),
                repository: Some(repo as Arc<dyn OrchestratorRepositoryPort>),
                ..OrchestratorConfig::default()
            },
            tx_clone,
        )
        .await
    });

    // Wait until the AwaitingApproval event arrives in the channel.
    let approval_id = loop {
        if let Ok(event) = rx.try_recv() {
            if let OrchestratorEvent::AwaitingApproval { approval_id, .. } = event {
                break approval_id;
            }
        } else {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    };

    // Approve the plan.
    assert!(registry.resolve(&approval_id, ApprovalDecision::Approve));

    // Collect remaining events and await executor.
    let result = exec_handle.await.unwrap();
    assert!(result.is_ok(), "execute() should succeed: {result:?}");

    // Re-drain the channel (events already collected above are gone; drain rest).
    drop(tx); // close sender so receiver terminates
    let remaining = collect_events(rx).await;

    // PlanApproved must appear in remaining events.
    let has_plan_approved = remaining
        .iter()
        .any(|e| matches!(e, OrchestratorEvent::PlanApproved));
    assert!(
        has_plan_approved,
        "PlanApproved missing; got: {remaining:?}"
    );

    // OrchestratorComplete must appear.
    let has_complete = remaining
        .iter()
        .any(|e| matches!(e, OrchestratorEvent::OrchestratorComplete { .. }));
    assert!(
        has_complete,
        "OrchestratorComplete missing; got: {remaining:?}"
    );
}

/// Gate: `ApprovePlan` — reject → `execute()` returns `PlanRejected` error.
#[tokio::test]
async fn hitl_reject_plan_aborts_run() {
    // LLM sequence: CoS → director → [gate: reject] — no execution calls needed.
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&one_node_plan_json()),
    ]);

    let registry = Arc::new(OrchestratorApprovalRegistry::new());
    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);

    let (tx, mut rx) = mpsc::channel(1024);

    let registry_clone = registry.clone();
    let tx_clone = tx.clone();
    let exec_handle = tokio::spawn(async move {
        execute(
            "Answer a question",
            &[],
            Arc::new(llm),
            tool_executor,
            OrchestratorConfig {
                hitl_mode: HitlMode::ApprovePlan,
                approval_registry: Some(
                    registry_clone as Arc<dyn OrchestratorApprovalRegistryPort>,
                ),
                ..OrchestratorConfig::default()
            },
            tx_clone,
        )
        .await
    });

    // Wait for the AwaitingApproval gate.
    let approval_id = loop {
        if let Ok(event) = rx.try_recv() {
            if let OrchestratorEvent::AwaitingApproval { approval_id, .. } = event {
                break approval_id;
            }
        } else {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    };

    // Reject the plan.
    assert!(registry.resolve(&approval_id, ApprovalDecision::Reject("bad plan".into())));

    let result = exec_handle.await.unwrap();
    drop(tx);
    let remaining = collect_events(rx).await;

    // execute() must return an error.
    assert!(result.is_err(), "execute() should fail on rejection");

    // PlanRejected event must appear.
    let has_rejected = remaining
        .iter()
        .any(|e| matches!(e, OrchestratorEvent::PlanRejected { .. }));
    assert!(
        has_rejected,
        "PlanRejected event missing; got: {remaining:?}"
    );
}

/// `mark_interrupted_runs()` transitions `Running` → `Interrupted` and leaves
/// other statuses unchanged.
#[tokio::test]
async fn interrupted_runs_marked_on_startup() {
    use gglib_core::domain::orchestrator::run::{OrchestratorRun, OrchestratorRunStatus};
    use gglib_core::domain::orchestrator::task_graph::HitlMode;

    let repo = make_repo().await;

    let make_run = |id: &str, status: OrchestratorRunStatus| OrchestratorRun {
        id: id.to_string(),
        goal: "test goal".to_string(),
        graph_json: None,
        status,
        hitl_mode: HitlMode::None,
        conversation_id: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    };

    // Insert two Running runs and one Completed run.
    repo.create_run(make_run("r1", OrchestratorRunStatus::Running))
        .await
        .unwrap();
    repo.create_run(make_run("r2", OrchestratorRunStatus::Running))
        .await
        .unwrap();
    repo.create_run(make_run("r3", OrchestratorRunStatus::Completed))
        .await
        .unwrap();

    let count = repo.mark_interrupted_runs().await.unwrap();
    assert_eq!(count, 2, "both Running runs should be marked interrupted");

    let r1 = repo.get_run("r1").await.unwrap().unwrap();
    let r2 = repo.get_run("r2").await.unwrap().unwrap();
    let r3 = repo.get_run("r3").await.unwrap().unwrap();

    assert!(
        matches!(r1.status, OrchestratorRunStatus::Interrupted),
        "r1 should be Interrupted"
    );
    assert!(
        matches!(r2.status, OrchestratorRunStatus::Interrupted),
        "r2 should be Interrupted"
    );
    assert!(
        matches!(r3.status, OrchestratorRunStatus::Completed),
        "r3 must remain Completed"
    );
}

/// `HitlMode::None` auto-approves the plan without blocking.
#[tokio::test]
async fn hitl_none_mode_auto_approves() {
    // LLM sequence: CoS → director → worker → compaction →
    //   synthesizer worker → synthesizer compaction → final synthesis
    let llm = StubLlm::new(vec![
        StubLlm::text_then_done(&cos_single_dept_json()),
        StubLlm::text_then_done(&one_node_plan_json()),
        StubLlm::text_then_done("The answer is 42."),
        StubLlm::text_then_done("Worker answered 42."),
        StubLlm::text_then_done("Synthesizer output."),
        StubLlm::text_then_done("Synthesizer compacted."),
        StubLlm::text_then_done("42."),
    ]);

    let tool_executor: Arc<dyn ToolExecutorPort> = Arc::new(EmptyToolExecutor);
    let (tx, rx) = mpsc::channel(1024);

    let result = execute(
        "Answer a simple question",
        &[],
        Arc::new(llm),
        tool_executor,
        OrchestratorConfig::default(), // HitlMode::None
        tx,
    )
    .await;

    assert!(result.is_ok(), "execute() should succeed: {result:?}");

    let events = collect_events(rx).await;

    // PlanApproved must be emitted (auto-approved).
    assert!(
        events
            .iter()
            .any(|e| matches!(e, OrchestratorEvent::PlanApproved)),
        "PlanApproved must appear in auto-approve mode; got: {events:?}"
    );

    // No AwaitingApproval should appear.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, OrchestratorEvent::AwaitingApproval { .. })),
        "AwaitingApproval must NOT appear in HitlMode::None; got: {events:?}"
    );

    // Must still complete.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, OrchestratorEvent::OrchestratorComplete { .. })),
        "OrchestratorComplete must appear; got: {events:?}"
    );
}
