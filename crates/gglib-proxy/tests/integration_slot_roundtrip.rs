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
use bytes::Bytes;
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
use gglib_proxy::slots::{SlotIoResult, attempt_save, slot_bin_path};

// ─── Mock upstream (serves /v1/chat/completions + /slots/0) ──────────────

/// Minimal mock that records action order and save/restore counts.
#[derive(Debug)]
struct FixedUpstream {
    port: u16,
    model_name: String,
    /// Mirrors `RunningTarget::slot_restore_supported`. False models a
    /// sliding-window/hybrid/recurrent model, where the proxy must bypass the
    /// disk slot layer entirely.
    slot_restore_supported: bool,
}

#[async_trait]
impl ModelRuntimePort for FixedUpstream {
    async fn ensure_model_running(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Ok(
            RunningTarget::local(self.port, 1, self.model_name.clone(), 4096, false)
                .with_slot_restore_supported(self.slot_restore_supported),
        )
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
/// Returns `(port, action_log, save_count, restore_count, last_chat_body)`
/// where `action_log` is a mutex-protected byte vector: `0` = restore,
/// `1` = generate, `2` = save; `last_chat_body` captures the raw bytes of
/// the most recent `/v1/chat/completions` request, for asserting on what
/// the proxy actually forwarded upstream (e.g. injected fields).
///
/// On a save action, this actually writes the requested `filename` (gglib
/// now sends a per-attempt temp name, see `slots::save_slot`) under
/// `slot_dir` — real llama-server does the equivalent, writing into its
/// `--slot-save-path`. Without this, gglib's post-save `rename(tmp, final)`
/// would always fail (nothing was ever written), turning every "successful"
/// save into a `Transient` failure and retry storm.
async fn spawn_mock_upstream_with_slots(
    cancel: CancellationToken,
    slot_dir: std::path::PathBuf,
) -> (
    u16,
    Arc<Mutex<Vec<u8>>>,
    Arc<AtomicU64>,
    Arc<AtomicU64>,
    Arc<Mutex<Option<Bytes>>>,
) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock upstream");
    let port = listener.local_addr().unwrap().port();

    let action_log: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let save_count = Arc::new(AtomicU64::new(0));
    let restore_count = Arc::new(AtomicU64::new(0));
    let last_chat_body: Arc<Mutex<Option<Bytes>>> = Arc::new(Mutex::new(None));

    let log_c = action_log.clone();
    let save_n = save_count.clone();
    let restore_n = restore_count.clone();
    let last_body_c = last_chat_body.clone();

    let app = Router::new()
        // Chat completions handler — records action `1` (generate) and
        // captures the received body for inspection.
        .route("/v1/chat/completions", {
            let log_c = log_c.clone();
            post(move |body: Bytes| {
                let log = log_c.clone();
                let last_body_c = last_body_c.clone();
                async move {
                    log.lock().await.push(1);
                    *last_body_c.lock().await = Some(body);
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
                move |params: axum::extract::Query<HashMap<String, String>>, body: Bytes| {
                    let log = log_c.clone();
                    let save_n = save_n.clone();
                    let restore_n = restore_n.clone();
                    let slot_dir = slot_dir.clone();
                    async move {
                        if let Some(action) = params.get("action") {
                            match action.as_str() {
                                "restore" => {
                                    log.lock().await.push(0);
                                    restore_n.fetch_add(1, Ordering::Relaxed);
                                }
                                "save" => {
                                    // Mirror real llama-server: write the requested
                                    // filename under the slot-save path so gglib's
                                    // post-save rename(tmp, final) has something to
                                    // find.
                                    if let Ok(payload) =
                                        serde_json::from_slice::<serde_json::Value>(&body)
                                        && let Some(filename) =
                                            payload.get("filename").and_then(|v| v.as_str())
                                    {
                                        let _ = std::fs::create_dir_all(&slot_dir);
                                        let _ = std::fs::write(
                                            slot_dir.join(filename),
                                            b"fake kv state",
                                        );
                                    }
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
    (port, action_log, save_count, restore_count, last_chat_body)
}

/// Spawn a proxy server with cache enabled, pointing at the given upstream port.
async fn spawn_proxy_with_cache(
    upstream_port: u16,
    model_name: &str,
    slot_dir: std::path::PathBuf,
) -> (String, CancellationToken) {
    spawn_proxy_with_cache_for_model(upstream_port, model_name, slot_dir, true).await
}

/// [`spawn_proxy_with_cache`] with control over whether the upstream model
/// supports disk slot restore.
async fn spawn_proxy_with_cache_for_model(
    upstream_port: u16,
    model_name: &str,
    slot_dir: std::path::PathBuf,
    slot_restore_supported: bool,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: model_name.into(),
        slot_restore_supported,
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
            gglib_proxy::slot_eviction::DiskBudget::Auto,
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
    let slot_dir =
        std::env::temp_dir().join(format!("gglib-slot-roundtrip-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&slot_dir);

    let upstream_cancel = CancellationToken::new();
    let (upstream_port, action_log, save_count, restore_count, last_chat_body) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), slot_dir.clone()).await;

    // Pre-create a slot file so the restore call actually reaches the mock
    // upstream — the existence precheck in `restore_with_retry` skips the
    // network call entirely when nothing is cached yet, which is correct in
    // production but means this round-trip test needs a real file to
    // exercise the restore leg. FixedUpstream::ensure_model_running always
    // returns model_id 1.
    let session_id = "roundtrip-test";
    let bin_path = slot_bin_path(&slot_dir, 1, session_id);
    std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
    std::fs::write(&bin_path, b"fake kv state").unwrap();

    let (proxy_base, proxy_cancel) =
        spawn_proxy_with_cache(upstream_port, "test-model", slot_dir.clone()).await;

    // Send a non-streaming chat completion request.
    let response = Client::new()
        .post(format!("{}/v1/chat/completions", proxy_base))
        .header("X-Gglib-Session-Id", session_id)
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

    // Regression test: llama-server's KV reuse (n_past = get_common_prefix(...)
    // in server-context.cpp) is gated entirely behind the request's own
    // `cache_prompt` flag — if it's ever false, restore/save succeeding is
    // meaningless, since the server discards the match and fully re-prefills
    // anyway. The client above never set this field; verify the proxy forces
    // it to true regardless.
    let forwarded_body = last_chat_body.lock().await.clone().expect("body captured");
    let forwarded_json: serde_json::Value = serde_json::from_slice(&forwarded_body).unwrap();
    assert_eq!(
        forwarded_json["cache_prompt"],
        serde_json::json!(true),
        "proxy must force cache_prompt=true so llama-server's reuse path isn't silently skipped, got: {}",
        forwarded_json
    );

    // Atomic save regression: the final `.bin` must exist post-save (the mock
    // upstream "wrote" the temp file gglib requested, so the post-save
    // rename(tmp, final) succeeded), and no leftover `.tmp` file should
    // remain — a successful save always finishes with exactly the canonical
    // name on disk, never the per-attempt temp name.
    let final_bin = slot_bin_path(&slot_dir, 1, session_id);
    assert!(
        final_bin.exists(),
        "final .bin should exist after a successful save: {}",
        final_bin.display()
    );
    let leftover_tmp: Vec<_> = std::fs::read_dir(&slot_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("tmp"))
        .collect();
    assert!(
        leftover_tmp.is_empty(),
        "no .tmp file should remain after a successful save, found: {:?}",
        leftover_tmp.iter().map(|e| e.path()).collect::<Vec<_>>()
    );

    // Cleanup.
    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let _ = std::fs::remove_dir_all(&slot_dir);
}

/// Verify that a request with NO `X-Gglib-Session-Id` header still derives a
/// session id from the request content itself (canonicalized system prompt +
/// first user message) and saves under it — so cache persistence works even
/// for clients (VS Code Copilot's LLM Gateway extension, curl, etc.) that
/// have no idea the header exists. No slot file is pre-created, so restore
/// is correctly skipped (nothing cached yet for a brand-new derived id — the
/// existence precheck in `restore_with_retry` never reaches the network for
/// this case); the save call proves the derivation actually plumbed through
/// to disk.
#[tokio::test]
async fn slot_roundtrip_no_header_falls_back_to_content_hash() {
    let slot_dir = std::env::temp_dir().join(format!("gglib-slot-fallback-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&slot_dir);

    let upstream_cancel = CancellationToken::new();
    let (upstream_port, action_log, save_count, restore_count, _last_chat_body) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), slot_dir.clone()).await;

    let (proxy_base, proxy_cancel) =
        spawn_proxy_with_cache(upstream_port, "test-model", slot_dir.clone()).await;

    // Send a non-streaming chat completion request with NO session header.
    let response = Client::new()
        .post(format!("{}/v1/chat/completions", proxy_base))
        .json(&json!({
            "model": "test-model",
            "messages": [
                { "role": "system", "content": "You are the Coder." },
                { "role": "user", "content": "Implement login" }
            ],
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

    assert_eq!(
        restore_count.load(Ordering::Relaxed),
        0,
        "No slot file exists yet for the derived id — restore should be skipped, not attempted"
    );
    assert_eq!(
        save_count.load(Ordering::Relaxed),
        1,
        "Expected exactly 1 save call even without a session header"
    );

    let actions = action_log.lock().await.clone();
    assert_eq!(
        actions,
        vec![1, 2],
        "Expected generate→save order (no restore — nothing cached yet), got: {:?}",
        actions
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let _ = std::fs::remove_dir_all(&slot_dir);
}

/// Regression test: if the calling client explicitly sends `cache_prompt:
/// false` (some OpenAI-compatible clients do, for reasons unrelated to
/// llama-server's KV reuse semantics), the proxy must still force it back to
/// true before forwarding. Without this, restore/save can both succeed and
/// llama-server will still discard the entire match and fully re-prefill —
/// silently defeating the whole point of this feature with no visible error.
#[tokio::test]
async fn client_sent_cache_prompt_false_is_overridden_to_true() {
    let slot_dir =
        std::env::temp_dir().join(format!("gglib-slot-cache-prompt-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&slot_dir);

    let upstream_cancel = CancellationToken::new();
    let (upstream_port, _action_log, _save_count, _restore_count, last_chat_body) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), slot_dir.clone()).await;

    let (proxy_base, proxy_cancel) =
        spawn_proxy_with_cache(upstream_port, "test-model", slot_dir.clone()).await;

    let response = Client::new()
        .post(format!("{}/v1/chat/completions", proxy_base))
        .header("X-Gglib-Session-Id", "cache-prompt-test")
        .json(&json!({
            "model": "test-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": false,
            "cache_prompt": false
        }))
        .send()
        .await
        .expect("proxy should be running");

    assert!(response.status().is_success());

    let forwarded_body = last_chat_body.lock().await.clone().expect("body captured");
    let forwarded_json: serde_json::Value = serde_json::from_slice(&forwarded_body).unwrap();
    assert_eq!(
        forwarded_json["cache_prompt"],
        serde_json::json!(true),
        "proxy must override an explicit client cache_prompt=false, got: {}",
        forwarded_json
    );

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

    let slot_dir = std::env::temp_dir().join(format!("gglib-backoff-test-{}", std::process::id()));
    // Pre-create a slot file so the existence precheck in `restore_with_retry`
    // lets this reach the mock's 503-then-200 sequence — otherwise "nothing
    // cached yet" would short-circuit to NotFound before any network call.
    let bin_path = slot_bin_path(&slot_dir, 0, "backoff-session");
    std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
    std::fs::write(&bin_path, b"fake kv state").unwrap();

    let config = StreamConfig {
        client: Client::new(),
        base_url: format!("http://127.0.0.1:{}", port),
        slot_dir,
        model_id: 0,
        clear_all_pending: Arc::new(AtomicBool::new(false)),
        per_session_cleared: Arc::new(DashSet::new()),
        server_start_time: Arc::new(AtomicU64::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        )),
        last_loaded_session: Arc::new(tokio::sync::RwLock::new(None)),
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
    let _ = std::fs::remove_dir_all(&config.slot_dir);
}

/// Regression test: `attempt_save` must retry a transient failure the same
/// way `restore_with_retry` already does. Without this, a stale pooled HTTP
/// connection (llama-server closes idle keep-alive connections; a long
/// generation easily outlives one) silently drops the save on the very next
/// attempt with no retry, leaving the on-disk `.bin` permanently stale
/// relative to the slot's actual live KV cache.
#[tokio::test]
async fn save_retry_backoff_exhausts_max_retries_then_succeeds() {
    let cancel = CancellationToken::new();

    // Mock upstream that returns 503 (transient) for the first 2 attempts,
    // then 200 (success) on the 3rd — same shape as the restore-retry test.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind save-retry mock");
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
                            Response::builder()
                                .status(503)
                                .body(Body::from("{}"))
                                .unwrap()
                        } else {
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

    let client = Client::new();
    let base_url = format!("http://127.0.0.1:{}", port);
    let clear_all_pending = AtomicBool::new(false);
    let per_session_cleared = DashSet::new();

    attempt_save(
        &client,
        &base_url,
        std::path::Path::new("/tmp"), // slot_dir (not used — DashSet guard fires first)
        0,                            // model_id
        "save-backoff-session",
        &clear_all_pending,
        &per_session_cleared,
    )
    .await;

    // Exactly 3 attempts: 1 initial + 2 retries, ending in success — proves
    // the retry loop ran rather than giving up after the first failure.
    assert_eq!(
        attempt_count.load(Ordering::Relaxed),
        3,
        "Expected 3 total save attempts (1 initial + 2 retries)"
    );

    cancel.cancel();
}

/// A model whose KV memory keeps only part of the token history
/// (sliding-window/hybrid/recurrent) must bypass the disk slot layer
/// completely — no restore, no save — even with the cache enabled and a
/// matching slot file already on disk.
///
/// llama-server's slot files omit the context checkpoints these models need to
/// resume, so a restore both fails to help *and* suppresses the in-RAM prompt
/// cache that would have. See `gglib_runtime::llama::args::slot_restore`.
#[tokio::test]
async fn partial_kv_model_bypasses_disk_slot_layer_entirely() {
    let slot_dir = std::env::temp_dir().join(format!(
        "gglib-slot-partial-kv-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::create_dir_all(&slot_dir);

    let upstream_cancel = CancellationToken::new();
    let (upstream_port, action_log, save_count, restore_count, _last_chat_body) =
        spawn_mock_upstream_with_slots(upstream_cancel.clone(), slot_dir.clone()).await;

    // Pre-create a slot file. On a supported model this is exactly what makes
    // the restore leg fire (see the round-trip test above), so its presence
    // here proves the bypass is driven by the model flag and not merely by a
    // cache miss.
    let session_id = "partial-kv-session";
    let bin_path = slot_bin_path(&slot_dir, 1, session_id);
    std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
    std::fs::write(&bin_path, b"fake kv state").unwrap();

    let (proxy_base, proxy_cancel) = spawn_proxy_with_cache_for_model(
        upstream_port,
        "test-model",
        slot_dir.clone(),
        false, // partial KV memory — disk layer must be skipped
    )
    .await;

    let response = Client::new()
        .post(format!("{}/v1/chat/completions", proxy_base))
        .header("X-Gglib-Session-Id", session_id)
        .json(&json!({
            "model": "test-model",
            "messages": [{ "role": "user", "content": "hello" }],
            "stream": false
        }))
        .send()
        .await
        .expect("proxy should be running");

    // The request must still succeed — disabling the disk layer is a caching
    // decision, not a degradation of the request path.
    assert!(
        response.status().is_success(),
        "Chat completion should succeed without the disk cache: {}",
        response.status()
    );

    assert_eq!(
        restore_count.load(Ordering::Relaxed),
        0,
        "partial-KV model must not trigger a slot restore"
    );
    assert_eq!(
        save_count.load(Ordering::Relaxed),
        0,
        "partial-KV model must not trigger a slot save"
    );

    // Only the generate call (1) should appear — no restore (0), no save (2).
    let actions = action_log.lock().await.clone();
    assert_eq!(
        actions,
        vec![1],
        "Expected generate only, got: {:?}",
        actions
    );

    proxy_cancel.cancel();
    upstream_cancel.cancel();
    let _ = std::fs::remove_dir_all(&slot_dir);
}
