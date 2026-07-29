//! Shared mock upstream + proxy-spawning rig for slot-cache integration tests.
//!
//! Extracted from `integration_slot_roundtrip.rs` so `cache_lifecycle.rs`
//! (the mtime-guard stale-skip test) can reuse the exact same mock
//! llama-server (`/v1/chat/completions` + `/slots/0`) and proxy-spawn
//! helpers instead of a second ad hoc copy.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use axum::{Router, body::Body, http::Response, routing::post};
use bytes::Bytes;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use gglib_core::domain::ModelCapabilities;
use gglib_core::ports::{
    CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelRuntimeError, ModelRuntimePort,
    ModelSummary, RunningTarget,
};

use super::common::{MockSettingsRepo, make_mcp_service, make_orchestrator_deps};

/// Minimal mock that records action order and save/restore counts.
#[derive(Debug)]
pub struct FixedUpstream {
    pub port: u16,
    pub model_name: String,
    /// Mirrors `RunningTarget::slot_restore_supported`. False models a
    /// sliding-window/hybrid/recurrent model, where the proxy must bypass the
    /// disk slot layer entirely.
    pub slot_restore_supported: bool,
    /// Whether this runtime reports itself pinned to `model_name`.
    ///
    /// Only affects [`ModelRuntimePort::pinned_model`] — enforcement lives in
    /// `gglib-runtime`'s `SwapState` and is out of scope here (see
    /// `integration_pinned_models.rs`). This exists so the roundtrip tests
    /// below can prove KV cache persistence works identically whether or not
    /// the endpoint is pinned — #633 asked for CI coverage of exactly this
    /// combination, and nothing previously exercised it.
    pub pinned: bool,
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

    fn pinned_model(&self) -> Option<&str> {
        self.pinned.then_some(self.model_name.as_str())
    }
}

/// Catalog that returns a single model with the given name.
#[derive(Debug)]
pub struct TaggedCatalog {
    pub name: String,
    pub tags: Vec<String>,
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
pub async fn spawn_mock_upstream_with_slots(
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
                    let body = serde_json::json!({
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
pub async fn spawn_proxy_with_cache(
    upstream_port: u16,
    model_name: &str,
    slot_dir: std::path::PathBuf,
) -> (String, CancellationToken) {
    spawn_proxy_with_cache_for_model(upstream_port, model_name, slot_dir, true, false).await
}

/// [`spawn_proxy_with_cache`], pinned to `model_name` — the `gglib serve`
/// shape, as opposed to every other helper here which models `gglib proxy`.
pub async fn spawn_pinned_proxy_with_cache(
    upstream_port: u16,
    model_name: &str,
    slot_dir: std::path::PathBuf,
) -> (String, CancellationToken) {
    spawn_proxy_with_cache_for_model(upstream_port, model_name, slot_dir, true, true).await
}

/// [`spawn_proxy_with_cache`] with control over whether the upstream model
/// supports disk slot restore, and whether the runtime reports itself pinned.
pub async fn spawn_proxy_with_cache_for_model(
    upstream_port: u16,
    model_name: &str,
    slot_dir: std::path::PathBuf,
    slot_restore_supported: bool,
    pinned: bool,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(FixedUpstream {
        port: upstream_port,
        model_name: model_name.into(),
        slot_restore_supported,
        pinned,
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(TaggedCatalog {
        name: model_name.into(),
        tags: vec![],
    });
    let mcp = make_mcp_service();
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
            Arc::new(MockSettingsRepo),
            None, // inference_override
            true, // cache_enabled
            Some(slot_dir),
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            &gglib_core::CorsConfig::LocalOnly,
        )
        .await
        .ok();
    });

    // Give the proxy time to start listening.
    tokio::time::sleep(Duration::from_millis(50)).await;
    (format!("http://{}", addr), cancel)
}
