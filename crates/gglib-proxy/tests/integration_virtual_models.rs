//! Integration tests for virtual model routing.
//!
//! Exercises all three virtual orchestrator model names through the full HTTP
//! stack (real `gglib_proxy::serve`) with scripted mock runners so we can
//! verify the SSE wire format without spawning an actual orchestrator.
//!
//! Covered:
//!
//! * `GET /v1/models` — all three virtual models appear in the listing.
//! * `POST /v1/chat/completions` with `gglib-orchestrator:native` → HTTP 400.
//! * Auto mode (`gglib-orchestrator`) — events are translated to markdown SSE.
//! * Interactive mode (`gglib-orchestrator:interactive`) — stream ends with the
//!   `<!-- gglib-run-id:… approval_id:… -->` sentinel on `AwaitingApproval`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use gglib_core::domain::orchestrator::events::{ApprovalKind, OrchestratorEvent};
use gglib_core::domain::orchestrator::run::{
    OrchestratorRun, OrchestratorRunEvent, OrchestratorRunStatus,
};
use gglib_core::domain::orchestrator::task_graph::{
    HitlMode, NodeId, NodeStatus, TaskGraph, TaskNode, TaskNodeKind,
};
use gglib_core::ports::{
    ApprovalDecision, CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError,
    ModelRuntimePort, ModelSummary, OrchestratorApprovalRegistryPort, OrchestratorRepositoryPort,
    RepositoryError, RunningTarget,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, NoopEmitter};
use gglib_mcp::McpService;
use gglib_proxy::{OrchestratorDeps, OrchestratorRunParams, OrchestratorRunnerPort};

// =============================================================================
// Minimal mock ports (runtime / catalog / MCP)
// =============================================================================

/// Runtime that always returns an error — virtual model requests never reach it.
#[derive(Debug)]
struct NoopRuntime;

#[async_trait]
impl ModelRuntimePort for NoopRuntime {
    async fn ensure_model_running(
        &self,
        model: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Err(ModelRuntimeError::ModelNotFound(model.to_string()))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Catalog that returns an empty model list.
#[derive(Debug)]
struct EmptyCatalog;

#[async_trait]
impl ModelCatalogPort for EmptyCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![])
    }
    async fn resolve_model(&self, _: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(None)
    }
    async fn resolve_for_launch(&self, _: &str) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

struct EmptyMcpRepo;

#[async_trait]
impl McpServerRepository for EmptyMcpRepo {
    async fn insert(&self, _s: NewMcpServer) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::Internal("not implemented".into()))
    }
    async fn get_by_id(&self, id: i64) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(id.to_string()))
    }
    async fn get_by_name(&self, name: &str) -> Result<McpServer, McpRepositoryError> {
        Err(McpRepositoryError::NotFound(name.into()))
    }
    async fn list(&self) -> Result<Vec<McpServer>, McpRepositoryError> {
        Ok(vec![])
    }
    async fn update(&self, _s: &McpServer) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn delete(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
    async fn update_last_connected(&self, _id: i64) -> Result<(), McpRepositoryError> {
        Ok(())
    }
}

// =============================================================================
// Scripted runner — emits a fixed sequence of OrchestratorEvents
// =============================================================================

/// A mock runner that emits a pre-configured sequence of events.
#[derive(Debug)]
struct ScriptedRunner {
    events: Vec<OrchestratorEvent>,
}

impl ScriptedRunner {
    fn new(events: Vec<OrchestratorEvent>) -> Self {
        Self { events }
    }
}

#[async_trait]
impl OrchestratorRunnerPort for ScriptedRunner {
    async fn run(
        &self,
        _goal: &str,
        _params: OrchestratorRunParams,
        tx: mpsc::Sender<OrchestratorEvent>,
        _cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        for event in &self.events {
            let _ = tx.send(event.clone()).await;
        }
        Ok(())
    }
}

// =============================================================================
// Noop orchestrator registry and repository
// =============================================================================

struct NoopApprovalRegistry;

impl OrchestratorApprovalRegistryPort for NoopApprovalRegistry {
    fn register(&self, _: String, _: oneshot::Sender<ApprovalDecision>) {}
    fn resolve(&self, _: &str, _: ApprovalDecision) -> bool {
        false
    }
    fn is_pending(&self, _: &str) -> bool {
        false
    }
}

struct NoopOrchestratorRepo;

#[async_trait]
impl OrchestratorRepositoryPort for NoopOrchestratorRepo {
    async fn create_run(&self, _: OrchestratorRun) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update_run_status(
        &self,
        _: &str,
        _: OrchestratorRunStatus,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update_graph(&self, _: &str, _: &str) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn append_event(&self, _: OrchestratorRunEvent) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn get_run(&self, _: &str) -> Result<Option<OrchestratorRun>, RepositoryError> {
        Ok(None)
    }
    async fn list_runs(
        &self,
        _: Option<OrchestratorRunStatus>,
    ) -> Result<Vec<OrchestratorRun>, RepositoryError> {
        Ok(vec![])
    }
    async fn list_events(&self, _: &str) -> Result<Vec<OrchestratorRunEvent>, RepositoryError> {
        Ok(vec![])
    }
    async fn mark_interrupted_runs(&self) -> Result<u64, RepositoryError> {
        Ok(0)
    }
}

// =============================================================================
// Proxy harness
// =============================================================================

async fn spawn_proxy_with(runner: Arc<dyn OrchestratorRunnerPort>) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mcp = Arc::new(McpService::new(
        Arc::new(EmptyMcpRepo),
        Arc::new(NoopEmitter::new()),
    ));
    let orchestrator = OrchestratorDeps {
        runner,
        approval_registry: Arc::new(NoopApprovalRegistry),
        orchestrator_repo: Arc::new(NoopOrchestratorRepo),
    };

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            mcp,
            orchestrator,
            cancel_clone,
        )
        .await
        .ok();
    });

    tokio::time::sleep(Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

/// Drain a streaming SSE response body into a list of `data:` payloads.
///
/// Returns the raw JSON strings (without the `data: ` prefix) for each
/// non-empty, non-`[DONE]` frame.
async fn collect_sse_data(resp: reqwest::Response) -> Vec<String> {
    let mut frames = Vec::new();
    let mut buf = String::new();
    let mut stream = resp.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let text = String::from_utf8_lossy(&chunk.unwrap()).into_owned();
        buf.push_str(&text);
        while let Some(pos) = buf.find("\n\n") {
            let frame = buf[..pos].trim().to_string();
            buf = buf[pos + 2..].to_string();
            if let Some(data) = frame.strip_prefix("data: ")
                && data != "[DONE]"
            {
                frames.push(data.to_string());
            }
        }
    }
    frames
}

/// Build a minimal one-node TaskGraph for tests.
fn test_graph() -> TaskGraph {
    TaskGraph::new(
        "Test goal".to_string(),
        HitlMode::None,
        vec![TaskNode {
            id: NodeId("n1".into()),
            goal: "step one".to_string(),
            depends_on: vec![],
            tool_allowlist: vec![],
            kind: TaskNodeKind::Leaf,
            role: None,
            status: NodeStatus::Pending,
            output: None,
            compacted_output: None,
            error: None,
        }],
    )
    .unwrap()
}

// =============================================================================
// Tests
// =============================================================================

/// `GET /v1/models` must include all three virtual model names.
#[tokio::test]
async fn test_models_endpoint_includes_virtual_models() {
    let runner = Arc::new(ScriptedRunner::new(vec![]));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .get(format!("{base}/v1/models"))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    let ids: Vec<&str> = body["data"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["id"].as_str())
        .collect();

    assert!(ids.contains(&"gglib-orchestrator"), "missing auto model");
    assert!(
        ids.contains(&"gglib-orchestrator:interactive"),
        "missing interactive model"
    );
    assert!(
        ids.contains(&"gglib-orchestrator:native"),
        "missing native model"
    );
    cancel.cancel();
}

/// `POST /v1/chat/completions` with `gglib-orchestrator:native` → HTTP 400.
#[tokio::test]
async fn test_native_mode_returns_400() {
    let runner = Arc::new(ScriptedRunner::new(vec![]));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": "gglib-orchestrator:native",
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    let msg = body["error"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("/api/orchestrator/run"),
        "expected redirect hint in 400 body, got: {msg}"
    );
    cancel.cancel();
}

/// Auto mode: `PlanProposed` + `NodeStarted` + `NodeTextDelta` + `SynthesisStart`
/// + `SynthesisTextDelta` + `OrchestratorComplete` produce the expected markdown
/// structure in the SSE stream.
#[tokio::test]
async fn test_auto_mode_streams_events_as_markdown() {
    let events = vec![
        OrchestratorEvent::PlanProposed {
            graph: test_graph(),
        },
        OrchestratorEvent::PlanApproved,
        OrchestratorEvent::NodeStarted {
            node_id: "n1".into(),
            goal: "step one".into(),
        },
        OrchestratorEvent::NodeTextDelta {
            node_id: "n1".into(),
            delta: "worker output".into(),
        },
        OrchestratorEvent::NodeComplete {
            node_id: "n1".into(),
            output_preview: "worker output".into(),
        },
        OrchestratorEvent::SynthesisStart,
        OrchestratorEvent::SynthesisTextDelta {
            delta: "final answer".into(),
        },
        OrchestratorEvent::OrchestratorComplete {
            answer: "final answer".into(),
        },
    ];

    let runner = Arc::new(ScriptedRunner::new(events));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": "gglib-orchestrator",
            "stream": true,
            "messages": [{"role": "user", "content": "do the thing"}],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );

    let frames = collect_sse_data(resp).await;
    assert!(!frames.is_empty(), "expected SSE frames, got none");

    // Concatenate all content deltas.
    let full_content: String = frames
        .iter()
        .filter_map(|f| {
            let v: Value = serde_json::from_str(f).ok()?;
            v["choices"][0]["delta"]["content"]
                .as_str()
                .map(str::to_string)
        })
        .collect();

    assert!(
        full_content.contains("## 🧭 Planning"),
        "missing planning header; got:\n{full_content}"
    );
    assert!(
        full_content.contains("## 🔧 Working on: step one"),
        "missing node header; got:\n{full_content}"
    );
    assert!(
        full_content.contains("worker output"),
        "missing worker delta; got:\n{full_content}"
    );
    assert!(
        full_content.contains("## 📝 Synthesizing"),
        "missing synthesis header; got:\n{full_content}"
    );
    assert!(
        full_content.contains("final answer"),
        "missing synthesis delta; got:\n{full_content}"
    );

    // The last data frame with a finish_reason must be "stop".
    let stop_frame = frames.iter().rev().find_map(|f| {
        let v: Value = serde_json::from_str(f).ok()?;
        let reason = v["choices"][0]["finish_reason"].as_str()?.to_string();
        Some(reason)
    });
    assert_eq!(
        stop_frame.as_deref(),
        Some("stop"),
        "last chunk did not have finish_reason=stop"
    );

    cancel.cancel();
}

/// Interactive mode first-turn: `AwaitingApproval` causes the stream to embed
/// the `<!-- gglib-run-id:… approval_id:… -->` sentinel and then stop.
#[tokio::test]
async fn test_interactive_mode_embeds_sentinel_on_awaiting_approval() {
    let events = vec![
        OrchestratorEvent::PlanProposed {
            graph: test_graph(),
        },
        OrchestratorEvent::AwaitingApproval {
            approval_id: "test-approval-id".into(),
            kind: ApprovalKind::Plan,
        },
    ];

    let runner = Arc::new(ScriptedRunner::new(events));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": "gglib-orchestrator:interactive",
            "stream": true,
            "messages": [{"role": "user", "content": "plan something"}],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let frames = collect_sse_data(resp).await;
    let full_content: String = frames
        .iter()
        .filter_map(|f| {
            let v: Value = serde_json::from_str(f).ok()?;
            v["choices"][0]["delta"]["content"]
                .as_str()
                .map(str::to_string)
        })
        .collect();

    assert!(
        full_content.contains("<!-- gglib-run-id:"),
        "missing run-id sentinel; got:\n{full_content}"
    );
    assert!(
        full_content.contains("approval_id:test-approval-id"),
        "missing approval_id sentinel; got:\n{full_content}"
    );
    assert!(
        full_content.contains("yes"),
        "missing approval prompt hint; got:\n{full_content}"
    );

    cancel.cancel();
}

/// Auto mode with no user message → HTTP 400.
#[tokio::test]
async fn test_auto_mode_rejects_empty_messages() {
    let runner = Arc::new(ScriptedRunner::new(vec![]));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": "gglib-orchestrator",
            "messages": [{"role": "system", "content": "you are helpful"}],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    cancel.cancel();
}
