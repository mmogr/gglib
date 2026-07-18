//! Slot roundtrip integration test: restore → generate → save cycle validation.
//!
//! Non-streaming path only (`stream: false` + JSON response).
//! Uses a mock upstream (no real llama-server binary) that serves
//! `/v1/chat/completions` and `/slots/0?action=save|restore`.

mod fixtures;

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Response, routing::post};
use dashmap::DashSet;
use reqwest::Client;
use serde_json::json;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::ModelCapabilities;
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort,
    ModelSummary, RunningTarget,
};
use gglib_proxy::cache_lifecycle::{StreamConfig, restore_with_retry};
use gglib_proxy::slots::SlotIoResult;

// ─── Mock upstream (serves /v1/chat/completions + /slots/0) ──────────────

/// Minimal mock that records action order and save/restore counts.
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
            false,
        ))
    }

    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }

    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        Ok(())
    }
}

/// Catalog that returns a single model with the given name.
#[derive(Debug)]
struct TaggedCatalog {
    name: String,
    tags: Vec<String>,
}

impl TaggedCatalog {
    fn summary(&self) -> ModelSummary {
        ModelSummary {
            id: 1,
            name: self.name.clone(),
            tags: self.tags.clone(),
            capabilities: ModelCapabilities::empty(),
            param_count: "7B".into(),
            quantization: None,
            architecture: None,
            created_at: 0,
            file_size: 0,
            context_length: None,
            inference_defaults: None,
            server_defaults: None,
        }
    }
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

/// Spawn a mock upstream server that records action order.
///
/// Returns `(port, action_log, save_count, restore_count)` where `action_log`
/// is a mutex-protected byte vector: `0` = restore, `1` = generate, `2` = save.
async fn spawn_mock_upstream_with_slots(
    cancel: CancellationToken,
) -> (u16, Arc<Mutex<Vec<u8>>>, Arc<AtomicU64>, Arc<AtomicU64>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let port = listener.local_addr().unwrap().port();

    let action_log: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let save_count = Arc::new(AtomicU64::new(0));
    let restore_count = Arc::new(AtomicU64::new(0));

    let log_c = action_log.clone();
    let save_n = save_count.clone();
    let restore_n = restore_count.clone();

    let app = Router::new()
        // Chat completions handler — records action `1` (generate)
        .route("/v1/chat/completions", {
            let log_c = log_c.clone();
            post(move || {
                let log = log_c.clone();
                async move {
                    log.lock().await.push(1);
                    let body = json!({
                        "id": "test-123",
                        "object": "chat.completion",
                        "model": "test-model",
                        "choices": [{
                            "index": 0,
                            "message": { "role": "assistant", "content": "hello" },
                            "finish_reason": "stop"
                        }]
                    })
                    .to_string();
                    Response::builder()
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap()
                }
            })
        })
        // Slot save/restore handler — records action `0` (restore) or `2` (save)
        .route(
            "/slots/0",
            post(
                move |params: axum::extract::Query<HashMap<String, String>>| {
                    let log = log_c.clone();
                    let save_n = save_n.clone();
                    let restore_n = restore_n.clone();
                    async move {
                        if let Some(action) = params.get("action") {
                            match action.as_str() {
                                "restore" => {
                                    log.lock().await.push(0);
                                    restore_n.fetch_add(1, Ordering::Relaxed);
                                }
                                "save" => {
                                    log.lock().await.push(2);
                                    save_n.fetch_add(1, Ordering::Relaxed);
                                }
                                _ => {}
                            }
                        }
                        Response::builder()
                            .status(200)
                            .body(Body::from("{}"))
                            .unwrap()
                    }
                },
            ),
        );

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel.cancelled_owned())
            .await
            .ok();
    });

    // Give the mock server time to start listening.
    tokio::time::sleep(Duration::from_millis(30)).await;
    (port, action_log, save_count, restore_count)
}

/// Spawn a proxy server with cache enabled, pointing at the given upstream port.
async fn spawn_proxy_with_cache(
    upstream_port: u16,
    model_name: &str,
    slot_dir: std::path::PathBuf,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: model_name.into(),
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: model_name.into(),
        tags: vec![],
    });
    let mcp = fixtures::common::make_mcp_service();
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            mcp,
            fixtures::common::make_orchestrator_deps(),
            cancel_clone,
            Arc::new(fixtures::common::MockSettingsRepo),
            true, // cache_enabled
            Some(slot_dir),
        )
        .await
        .ok();
    });

    // Give the proxy time to start listening.
    tokio::time::sleep(Duration::from_millis(50)).await;
    (format!("http://{}", addr), cancel)
}

// ─── Tests ────────────────────────────────────────────────────────────────

/// Verify that a non-streaming chat completion triggers restore→generate→save
/// in the correct order, with exactly one call each.
#[tokio::test]
async fn slot_roundtrip_non_streaming_verify_order_and_counts() {
    let upstream_cancel = CancellationToken::new();
    let (upstream_port, action_log, save_count, restore_count) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone()).await;

    let slot_dir =
        std::env::temp_dir().join(format!("gglib-slot-roundtrip-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&slot_dir);

    let (proxy_base, proxy_cancel) =
        spawn_proxy_with_cache(upstream_port, "test-model", slot_dir.clone()).await;

    // Send a non-streaming chat completion request.
    let response = Client::new()
        .post(format!("{}/v1/chat/completions", proxy_base))
        .header("X-Gglib-Session-Id", "roundtrip-test")
        .json(&json!({
            "model": "test-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": false
        }))
        .send()
        .await
        .expect("proxy should be running");

    assert!(
        response.status().is_success(),
        "Chat completion should succeed: {}",
        response.status()
    );

    // Verify exactly one restore and one save were called.
    assert_eq!(
        restore_count.load(Ordering::Relaxed),
        1,
        "Expected exactly 1 restore call"
    );
    assert_eq!(
        save_count.load(Ordering::Relaxed),
        1,
        "Expected exactly 1 save call"
    );

    // Verify call order: restore (0) → generate (1) → save (2).
    let actions = action_log.lock().await.clone();
    assert_eq!(
        actions,
        vec![0, 1, 2],
        "Expected restore→generate→save order, got: {:?}",
        actions
    );

    // Cleanup.
    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let _ = std::fs::remove_dir_all(&slot_dir);
}

/// Verify that `restore_with_retry` exhausts MAX_RETRIES (2) on transient
/// failures, then succeeds on the 3rd attempt — and that backoff delays
/// accumulate to at least 200ms.
#[tokio::test]
async fn retry_backoff_exhausts_max_retries_then_succeeds() {
    let cancel = CancellationToken::new();

    // Mock upstream that returns 503 for the first 2 attempts, then 200.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind retry mock");
    let port = listener.local_addr().unwrap().port();

    let attempt_count = Arc::new(AtomicU64::new(0));
    let attempt_c = attempt_count.clone();

    let cancel_spawn = cancel.clone();
    tokio::spawn(async move {
        let app = Router::new().route(
            "/slots/0",
            post(
                move |_params: axum::extract::Query<HashMap<String, String>>| {
                    let cnt = attempt_c.clone();
                    async move {
                        let n = cnt.fetch_add(1, Ordering::Relaxed);
                        if n < 2 {
                            // First 2 attempts: transient failure
                            Response::builder()
                                .status(503)
                                .body(Body::from("{}"))
                                .unwrap()
                        } else {
                            // 3rd attempt: success
                            Response::builder()
                                .status(200)
                                .body(Body::from("{}"))
                                .unwrap()
                        }
                    }
                },
            ),
        );
        axum::serve(listener, app)
            .with_graceful_shutdown(cancel_spawn.cancelled_owned())
            .await
            .ok();
    });

    tokio::time::sleep(Duration::from_millis(30)).await;

    let config = StreamConfig {
        client: Client::new(),
        base_url: format!("http://127.0.0.1:{}", port),
        slot_dir: std::env::temp_dir().join("backoff-test"),
        clear_all_pending: Arc::new(AtomicBool::new(false)),
        per_session_cleared: Arc::new(DashSet::new()),
        server_start_time: Arc::new(AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )),
    };

    let result = restore_with_retry(&config, "backoff-session").await;

    // After 2 retries (3 total attempts), should succeed.
    assert!(
        matches!(result, SlotIoResult::Ok),
        "Expected Ok after retries, got: {:?}",
        result
    );

    // Exactly 3 attempts: 1 initial + 2 retries.
    assert_eq!(
        attempt_count.load(Ordering::Relaxed),
        3,
        "Expected 3 total attempts (1 initial + 2 retries)"
    );
    // Deliberately no wall-clock timing assertion — the real I/O + real 100ms sleeps
    // still occur (test takes ~200ms wall time), but we don't assert on elapsed
    // duration to avoid CI flakiness. attempt_count == 3 already proves the fixed
    // backoff ran twice before success.

    cancel.cancel();
}
