//! End-to-end round-trip tests for the proxy normalization pipeline.
//!
//! Each test:
//!
//! 1. Spins up a **mock upstream** HTTP server that streams a fixture's bytes
//!    in response to `POST /v1/chat/completions`.
//! 2. Spins up the **real `gglib-proxy`** with mock ports — the runtime
//!    points at the mock upstream and the catalog returns a `ModelSummary`
//!    whose `tags` select the dialect parser under test.
//! 3. Sends a streaming chat-completion request from a strict external
//!    client (plain `reqwest`).
//! 4. Collects the proxy's response bytes and parses every `data:` frame as
//!    JSON.
//! 5. Asserts the **post-normalization** wire format — the bytes external
//!    clients (OpenWebUI, OpenAI SDKs) would actually see.
//!
//! No `gglib_core::sse::*` types are used in the assertions; the tests speak
//! pure HTTP + JSON, exactly like an external consumer.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, response::Response, routing::post};
use bytes::Bytes;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::council::{CouncilEvent, CouncilRun, CouncilRunEvent, CouncilRunStatus};
use gglib_core::ports::{
    ApprovalDecision, CouncilApprovalRegistryPort, CouncilRepositoryPort, RepositoryError,
};
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort,
    ModelSummary, RunningTarget,
};
use gglib_core::{McpRepositoryError, McpServer, McpServerRepository, NewMcpServer, NoopEmitter};
use gglib_mcp::McpService;
use gglib_proxy::{CouncilDeps, CouncilRunParams, CouncilRunnerPort};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
struct NoopRunner;
#[async_trait]
impl CouncilRunnerPort for NoopRunner {
    async fn run(
        &self,
        _: &str,
        _: CouncilRunParams,
        _: mpsc::Sender<CouncilEvent>,
        _: CancellationToken,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
struct NoopApprovalRegistry;
impl CouncilApprovalRegistryPort for NoopApprovalRegistry {
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
impl CouncilRepositoryPort for NoopOrchestratorRepo {
    async fn create_run(&self, _: CouncilRun) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update_run_status(&self, _: &str, _: CouncilRunStatus) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update_graph(&self, _: &str, _: &str) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn append_event(&self, _: CouncilRunEvent) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn get_run(&self, _: &str) -> Result<Option<CouncilRun>, RepositoryError> {
        Ok(None)
    }
    async fn list_runs(
        &self,
        _: Option<CouncilRunStatus>,
    ) -> Result<Vec<CouncilRun>, RepositoryError> {
        Ok(vec![])
    }
    async fn list_events(&self, _: &str) -> Result<Vec<CouncilRunEvent>, RepositoryError> {
        Ok(vec![])
    }
    async fn truncate_events_after_wave(&self, _: &str, _: u32) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn mark_interrupted_runs(&self) -> Result<u64, RepositoryError> {
        Ok(0)
    }
}
fn make_orchestrator_deps() -> CouncilDeps {
    CouncilDeps {
        runner: Arc::new(NoopRunner),
        approval_registry: Arc::new(NoopApprovalRegistry),
        council_repo: Arc::new(NoopOrchestratorRepo),
    }
}

mod fixtures;
use fixtures::sse::{
    BASIC_TEXT, MALFORMED_JSON_RECOVERY, QWEN_XML_TOOL_CALL, REASONING_DEEPSEEK,
    STANDARD_OPENAI_TOOL_CALL, basic_text_split_chunks,
};

// ─── Mock ports ────────────────────────────────────────────────────────────

/// Runtime port that hands back a fixed upstream port.
#[derive(Debug)]
struct FixedUpstream {
    port: u16,
    model_name: String,
}

#[async_trait]
impl ModelRuntimePort for FixedUpstream {
    async fn ensure_model_running(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Ok(RunningTarget::local(
            self.port,
            1,
            self.model_name.clone(),
            4096,
        ))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Catalog port that always resolves the requested model with the given tags.
#[derive(Debug)]
struct TaggedCatalog {
    name: String,
    tags: Vec<String>,
}

#[async_trait]
impl ModelCatalogPort for TaggedCatalog {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(vec![self.summary()])
    }
    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        if name == self.name {
            Ok(Some(self.summary()))
        } else {
            Ok(None)
        }
    }
    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

impl TaggedCatalog {
    fn summary(&self) -> ModelSummary {
        ModelSummary {
            id: 1,
            name: self.name.clone(),
            tags: self.tags.clone(),
            capabilities: gglib_core::domain::ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            inference_defaults: None,
        }
    }
}

/// Empty MCP repo — chat completion path doesn't touch MCP, but the proxy
/// still wires it up.
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

// ─── Mock upstream ─────────────────────────────────────────────────────────

/// Spawn a mock upstream HTTP server that yields `chunks` (in order) when
/// `POST /v1/chat/completions` is called.  Returns the bound port.
///
/// Each chunk is sent as a separate body frame so tests can deliberately
/// split SSE frames across byte boundaries.
async fn spawn_mock_upstream(chunks: Vec<&'static [u8]>, cancel: CancellationToken) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();

    // Wrap chunks in a Mutex<Option<...>> so the handler can take them once
    // (axum requires Fn handlers; we serve a single request per upstream).
    let slot: Arc<Mutex<Option<Vec<&'static [u8]>>>> = Arc::new(Mutex::new(Some(chunks)));

    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let slot = slot.clone();
            async move {
                let chunks = slot
                    .lock()
                    .unwrap()
                    .take()
                    .unwrap_or_else(|| vec![b"data: [DONE]\n\n" as &[u8]]);
                let stream = futures_util::stream::iter(
                    chunks
                        .into_iter()
                        .map(|c| Ok::<Bytes, std::io::Error>(Bytes::from_static(c))),
                );
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .body(Body::from_stream(stream))
                    .unwrap()
            }
        }),
    );

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .ok();
    });

    // Give the listener a moment to start accepting.
    tokio::time::sleep(Duration::from_millis(30)).await;
    port
}

// ─── Proxy harness ─────────────────────────────────────────────────────────

/// Spawn the real `gglib_proxy::serve` with the given upstream port and
/// dialect tags.  Returns `(proxy_base_url, cancel)`.
async fn spawn_proxy(
    upstream_port: u16,
    model_name: &str,
    tags: Vec<String>,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: model_name.into(),
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: model_name.into(),
        tags,
    });
    let mcp = Arc::new(McpService::new(
        Arc::new(EmptyMcpRepo),
        Arc::new(NoopEmitter::new()),
    ));

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            mcp,
            make_orchestrator_deps(),
            cancel_clone,
        )
        .await
        .ok();
    });

    tokio::time::sleep(Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

// ─── End-to-end driver ─────────────────────────────────────────────────────

/// Send a streaming chat-completion request to the proxy and collect the
/// raw response bytes.
async fn round_trip(
    upstream_chunks: Vec<&'static [u8]>,
    model_name: &str,
    tags: Vec<String>,
) -> String {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(upstream_chunks, upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) = spawn_proxy(upstream_port, model_name, tags).await;

    let client = Client::new();
    let resp = client
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({
            "model": model_name,
            "stream": true,
            "messages": [{"role": "user", "content": "hi"}],
        }))
        .send()
        .await
        .expect("proxy request");

    assert_eq!(resp.status(), 200, "proxy returned non-200");
    assert_eq!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok()),
        Some("text/event-stream")
    );

    // Drain the streaming body into a single String — tests parse data:
    // frames out of it the same way an external client would.
    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        body.extend_from_slice(&chunk.expect("body chunk"));
    }

    proxy_cancel.cancel();
    upstream_cancel.cancel();

    String::from_utf8(body).expect("utf-8 body")
}

/// Parse `data:` payloads from the SSE-encoded body.  Returns one entry per
/// frame; `[DONE]` is included as a literal `String` distinguished by the
/// caller.
fn parse_frames(body: &str) -> (Vec<Value>, bool) {
    let mut frames = Vec::new();
    let mut saw_done = false;
    for raw in body.split("\n\n") {
        let line = raw.trim_start();
        let Some(payload) = line.strip_prefix("data: ") else {
            continue;
        };
        if payload.trim() == "[DONE]" {
            saw_done = true;
            continue;
        }
        let v: Value = serde_json::from_str(payload).unwrap_or_else(|e| {
            panic!("proxy emitted non-JSON data frame: {e}\nframe: {payload}");
        });
        frames.push(v);
    }
    (frames, saw_done)
}

/// Assert that every frame has the OpenAI canonical envelope and a stable
/// `id` / `model` / `created` triple.  Returns the (id, model, created).
fn assert_canonical_envelope(frames: &[Value], expected_model: &str) -> (String, String, u64) {
    assert!(!frames.is_empty(), "expected at least one data frame");
    let first = &frames[0];
    let id = first["id"].as_str().expect("string id").to_owned();
    let model = first["model"].as_str().expect("string model").to_owned();
    let created = first["created"].as_u64().expect("u64 created");

    assert!(
        id.starts_with("chatcmpl-"),
        "id should start with chatcmpl-, got {id}"
    );
    assert_eq!(model, expected_model, "advertised model name mismatch");

    for f in frames {
        // PromptProgress frames are top-level (no choices) — they still must
        // share the envelope identity.
        assert_eq!(f["object"], "chat.completion.chunk");
        assert_eq!(f["id"], json!(id), "id must be stable across frames");
        assert_eq!(
            f["model"],
            json!(model),
            "model must be stable across frames"
        );
        assert_eq!(
            f["created"],
            json!(created),
            "created must be stable across frames"
        );
    }

    (id, model, created)
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

/// Vanilla streaming text — the proxy must re-emit content deltas verbatim
/// and terminate with `data: [DONE]`.
#[tokio::test]
async fn basic_text_round_trip() {
    let body = round_trip(vec![BASIC_TEXT], "test-model", vec![]).await;
    let (frames, saw_done) = parse_frames(&body);
    assert!(saw_done, "missing [DONE] terminator");
    assert_canonical_envelope(&frames, "test-model");

    // Reconstruct the visible text from delta.content fields.
    let text: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, "Hello, world");

    // The stop chunk has finish_reason="stop" and an empty delta.
    let stop = frames
        .iter()
        .find(|f| f["choices"][0]["finish_reason"] == "stop")
        .expect("missing stop chunk");
    assert!(
        stop["choices"][0]["delta"]
            .as_object()
            .is_some_and(|o| o.is_empty()),
        "stop chunk should have empty delta, got {stop}"
    );
}

/// DeepSeek/QwQ-style reasoning frames must surface as `reasoning_content`
/// deltas, with text content following.
#[tokio::test]
async fn reasoning_content_round_trip() {
    let body = round_trip(vec![REASONING_DEEPSEEK], "r1-test", vec![]).await;
    let (frames, saw_done) = parse_frames(&body);
    assert!(saw_done);
    assert_canonical_envelope(&frames, "r1-test");

    let reasoning: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["reasoning_content"].as_str())
        .collect();
    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(reasoning, "Let me think.");
    assert_eq!(content, "42");
}

/// Qwen XML tool calls must be rewritten into strict OpenAI `tool_calls`
/// deltas — the `<tool_call>…</tool_call>` markers must NOT appear in the
/// rebuilt content stream.
#[tokio::test]
async fn qwen_xml_tool_call_is_normalized() {
    let body = round_trip(
        vec![QWEN_XML_TOOL_CALL],
        "qwen3-coder",
        vec!["format:qwen-xml".to_owned()],
    )
    .await;
    let (frames, saw_done) = parse_frames(&body);
    assert!(saw_done);
    assert_canonical_envelope(&frames, "qwen3-coder");

    // The literal Qwen markup MUST NOT appear in the wire output.
    assert!(
        !body.contains("<tool_call>") && !body.contains("</tool_call>"),
        "Qwen XML markers leaked into wire output:\n{body}"
    );

    // Reconstruct the visible content — should only contain the leading prose.
    let content: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(content, "Looking it up. ");

    // Find the tool_calls delta(s).
    let tc_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["choices"][0]["delta"]["tool_calls"].is_array())
        .collect();
    assert!(
        !tc_frames.is_empty(),
        "expected at least one tool_calls delta"
    );

    // First tool_call delta must carry id + type:"function" + function.name.
    let first_tc = &tc_frames[0]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(first_tc["index"], json!(0));
    assert!(
        first_tc["id"].is_string(),
        "first tool_call delta missing id"
    );
    assert_eq!(first_tc["type"], "function");
    assert_eq!(first_tc["function"]["name"], "get_weather");

    // The cumulative arguments JSON must reconstruct to the original args.
    let mut args = String::new();
    for f in &tc_frames {
        if let Some(s) = f["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"].as_str()
        {
            args.push_str(s);
        }
    }
    let parsed_args: Value =
        serde_json::from_str(&args).expect("tool_call arguments should be JSON");
    assert_eq!(parsed_args, json!({"city": "Paris"}));

    // Final chunk must announce finish_reason="tool_calls".
    let stop = frames
        .iter()
        .find(|f| f["choices"][0]["finish_reason"] == "tool_calls")
        .expect("missing tool_calls finish chunk");
    assert!(
        stop["choices"][0]["delta"]
            .as_object()
            .is_some_and(|o| o.is_empty())
    );
}

/// A standard OpenAI tool-call stream (no dialect) must round-trip through
/// the identity parser preserving id, type, name, and arguments.
#[tokio::test]
async fn standard_openai_tool_call_passthrough() {
    let body = round_trip(vec![STANDARD_OPENAI_TOOL_CALL], "strict-openai", vec![]).await;
    let (frames, saw_done) = parse_frames(&body);
    assert!(saw_done);
    assert_canonical_envelope(&frames, "strict-openai");

    let tc_frames: Vec<&Value> = frames
        .iter()
        .filter(|f| f["choices"][0]["delta"]["tool_calls"].is_array())
        .collect();
    assert!(!tc_frames.is_empty(), "expected tool_calls deltas");

    let first_tc = &tc_frames[0]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(first_tc["index"], json!(0));
    assert_eq!(first_tc["id"], "call_abc");
    assert_eq!(first_tc["type"], "function");
    assert_eq!(first_tc["function"]["name"], "get_weather");

    let mut args = String::new();
    for f in &tc_frames {
        if let Some(s) = f["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"].as_str()
        {
            args.push_str(s);
        }
    }
    let parsed_args: Value = serde_json::from_str(&args).expect("arguments should be JSON");
    assert_eq!(parsed_args, json!({"city": "Paris"}));
}

/// SSE frames split across arbitrary byte boundaries must reassemble inside
/// `SseStreamDecoder` and produce the same content as if they had arrived
/// whole.
#[tokio::test]
async fn split_frame_round_trip() {
    let body = round_trip(basic_text_split_chunks(), "split-model", vec![]).await;
    let (frames, saw_done) = parse_frames(&body);
    assert!(saw_done, "missing [DONE] terminator after split chunks");
    assert_canonical_envelope(&frames, "split-model");

    let text: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert_eq!(text, "split-frame-1-and-2");
}

/// A malformed `data:` payload in the upstream stream is unrecoverable at
/// the SSE-frame layer (we cannot know where the next frame begins), so the
/// proxy must:
/// 1. Emit any content frames that arrived **before** the malformed frame.
/// 2. Surface a structured `error` data frame so the client sees a terminal
///    signal instead of a hang.
/// 3. Always finish with `data: [DONE]`.
#[tokio::test]
async fn malformed_json_terminates_cleanly() {
    let body = round_trip(vec![MALFORMED_JSON_RECOVERY], "noisy-model", vec![]).await;
    let (frames, saw_done) = parse_frames(&body);
    assert!(saw_done, "missing [DONE] after malformed-json frame");

    // Pre-error content must have made it to the wire.
    let pre_error_text: String = frames
        .iter()
        .filter_map(|f| f["choices"][0]["delta"]["content"].as_str())
        .collect();
    assert!(
        pre_error_text.contains("before"),
        "content before malformed frame must be delivered, got {pre_error_text:?}"
    );

    // The terminator must be a structured error frame, not a half-emitted
    // chunk-completion.  This proves the client gets a clean signal instead
    // of a silent hang.
    let error_frame = frames
        .iter()
        .find(|f| f.get("error").is_some())
        .expect("expected a structured error frame on malformed JSON");
    assert!(
        error_frame["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "error frame missing message"
    );
}
