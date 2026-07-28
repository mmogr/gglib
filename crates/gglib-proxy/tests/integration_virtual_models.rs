//! Integration tests for virtual model routing.
//!
//! Exercises all three virtual orchestrator model names through the full HTTP
//! stack (real `gglib_proxy::serve`) with scripted mock runners so we can
//! verify the SSE wire format without spawning an actual orchestrator.
//!
//! Covered:
//!
//! * `GET /v1/models` — all three virtual models appear in the listing.
//! * `POST /v1/chat/completions` with `gglib-council:native` → HTTP 400.
//! * Auto mode (`gglib-council`) — events are translated to markdown SSE.
//! * Interactive mode (`gglib-council:interactive`) — stream ends with the
//!   `<!-- gglib-run-id:… approval_id:… -->` sentinel on `AwaitingApproval`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::council::events::{ApprovalKind, CouncilEvent};
use gglib_core::domain::council::task_graph::{
    HitlMode, NodeId, NodeStatus, TaskGraph, TaskNode, TaskNodeKind,
};
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use gglib_proxy::{CouncilDeps, CouncilRunParams, CouncilRunnerPort};

mod fixtures;
use fixtures::common::{
    EmptyCatalog, MockSettingsRepo, NoopApprovalRegistry, NoopOrchestratorRepo, NoopRuntime,
    make_mcp_service,
};

// =============================================================================
// Scripted runner — emits a fixed sequence of CouncilEvents
// =============================================================================

/// A mock runner that emits a pre-configured sequence of events.
#[derive(Debug)]
struct ScriptedRunner {
    events: Vec<CouncilEvent>,
}

impl ScriptedRunner {
    fn new(events: Vec<CouncilEvent>) -> Self {
        Self { events }
    }
}

#[async_trait]
impl CouncilRunnerPort for ScriptedRunner {
    async fn run(
        &self,
        _goal: &str,
        _params: CouncilRunParams,
        tx: mpsc::Sender<CouncilEvent>,
        _cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        for event in &self.events {
            let _ = tx.send(event.clone()).await;
        }
        Ok(())
    }
}

// =============================================================================
// Proxy harness
// =============================================================================

async fn spawn_proxy_with(runner: Arc<dyn CouncilRunnerPort>) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let mcp = make_mcp_service();
    let orchestrator = CouncilDeps {
        runner,
        approval_registry: Arc::new(NoopApprovalRegistry),
        council_repo: Arc::new(NoopOrchestratorRepo),
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
            Arc::new(MockSettingsRepo),
            None, // inference_override
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            &gglib_core::CorsConfig::LocalOnly,
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

    assert!(ids.contains(&"gglib-council"), "missing auto model");
    assert!(
        ids.contains(&"gglib-council:interactive"),
        "missing interactive model"
    );
    assert!(
        ids.contains(&"gglib-council:native"),
        "missing native model"
    );
    cancel.cancel();
}

/// `POST /v1/chat/completions` with `gglib-council:native` → HTTP 400.
#[tokio::test]
async fn test_native_mode_returns_400() {
    let runner = Arc::new(ScriptedRunner::new(vec![]));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": "gglib-council:native",
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    let msg = body["error"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("/api/council/run"),
        "expected redirect hint in 400 body, got: {msg}"
    );
    cancel.cancel();
}

/// Auto mode: `PlanProposed` + `NodeStarted` + `NodeTextDelta` + `SynthesisStart`
/// + `SynthesisTextDelta` + `CouncilComplete` produce the expected markdown
/// structure in the SSE stream.
#[tokio::test]
async fn test_auto_mode_streams_events_as_markdown() {
    let events = vec![
        CouncilEvent::PlanProposed {
            graph: test_graph(),
        },
        CouncilEvent::PlanApproved,
        CouncilEvent::NodeStarted {
            node_id: "n1".into(),
            goal: "step one".into(),
        },
        CouncilEvent::NodeTextDelta {
            node_id: "n1".into(),
            delta: "worker output".into(),
        },
        CouncilEvent::NodeComplete {
            node_id: "n1".into(),
            output_preview: "worker output".into(),
        },
        CouncilEvent::SynthesisStart,
        CouncilEvent::SynthesisTextDelta {
            delta: "final answer".into(),
        },
        CouncilEvent::CouncilComplete {
            answer: "final answer".into(),
        },
    ];

    let runner = Arc::new(ScriptedRunner::new(events));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": "gglib-council",
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
        CouncilEvent::PlanProposed {
            graph: test_graph(),
        },
        CouncilEvent::AwaitingApproval {
            approval_id: "test-approval-id".into(),
            kind: ApprovalKind::Plan,
        },
    ];

    let runner = Arc::new(ScriptedRunner::new(events));
    let (base, cancel) = spawn_proxy_with(runner).await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": "gglib-council:interactive",
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
            "model": "gglib-council",
            "messages": [{"role": "system", "content": "you are helpful"}],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    cancel.cancel();
}
