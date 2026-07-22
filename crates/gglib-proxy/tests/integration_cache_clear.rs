//! Integration tests for `POST /v1/proxy/cache/clear`.
//!
//! Spawns the real `gglib_proxy::serve` (not a hand-rolled router), sharing
//! its mock ports with `integration_slot_roundtrip.rs` via `tests/fixtures`
//! rather than duplicating them.

mod fixtures;

use std::sync::Arc;

use reqwest::Client;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_core::ports::ModelCatalogPort;
use gglib_core::ports::{ModelRuntimeError, ModelRuntimePort, RunningTarget};

/// Runtime port that counts `stop_current()` calls, so a test can assert the
/// model was actually recycled — the only way to drop llama-server's host-RAM
/// prompt cache.
#[derive(Debug, Default)]
struct RecordingRuntime {
    stops: Arc<AtomicUsize>,
}

#[async_trait]
impl ModelRuntimePort for RecordingRuntime {
    async fn ensure_model_running(
        &self,
        _model_name: &str,
        _num_ctx: Option<u64>,
        _default_ctx: u64,
    ) -> Result<RunningTarget, ModelRuntimeError> {
        Ok(RunningTarget::local(0, 1, "mock".into(), 4096, false))
    }
    async fn current_model(&self) -> Option<RunningTarget> {
        None
    }
    async fn stop_current(&self) -> Result<(), ModelRuntimeError> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

// ─── Proxy harness ─────────────────────────────────────────────────────────

/// Spawn the real `gglib_proxy::serve` with the given cache settings.
/// Returns `(proxy_base_url, cancel_token, stop_current_counter)`.
async fn spawn_proxy(
    cache_enabled: bool,
    slot_dir: Option<std::path::PathBuf>,
) -> (String, CancellationToken, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let stops = Arc::new(AtomicUsize::new(0));
    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(RecordingRuntime {
        stops: Arc::clone(&stops),
    });
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(fixtures::common::EmptyCatalog);
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
            None, // inference_override
            cache_enabled,
            slot_dir,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
        )
        .await
        .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel, stops)
}

// ─── Tests ──────────────────────────────────────────────────────────────

/// With the disk layer off — the common configuration — a global clear must
/// still recycle the model, because that is the only way to drop llama-server's
/// host-RAM prompt cache. Previously this reported "cache not enabled" and did
/// nothing at all, leaving the only cache actually in use unclearable.
#[tokio::test]
async fn cache_clear_recycles_the_model_when_disk_cache_is_disabled() {
    let (base_url, cancel, stops) = spawn_proxy(false, None).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert_eq!(body["status"], "ok");
    assert_eq!(body["disk"], "disk cache not enabled");
    assert_eq!(
        stops.load(Ordering::SeqCst),
        1,
        "a global clear must recycle the model to flush the RAM cache"
    );

    cancel.cancel();
}

/// A session-scoped clear is disk-only: recycling the process to service one
/// session would discard every other session's cached prefix too.
#[tokio::test]
async fn cache_clear_with_session_id_does_not_recycle_the_model() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel, stops) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .header("X-Gglib-Session-Id", "some_session")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    assert_eq!(
        stops.load(Ordering::SeqCst),
        0,
        "session-scoped clear must leave the shared RAM cache alone"
    );

    cancel.cancel();
}

/// A session ID that looks like a path-traversal attempt should be rejected
/// with 400 Bad Request before any filesystem access.
#[tokio::test]
async fn cache_clear_returns_400_on_invalid_session_id() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel, _stops) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .header("x-gglib-session-id", "../etc/passwd")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 400);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert!(
        body["error"]
            .as_str()
            .expect("error is string")
            .contains("invalid session id")
    );

    cancel.cancel();
}

/// Without a session header the handler should clear all slots and return
/// 200 with an "all slots cleared" message.
#[tokio::test]
async fn cache_clear_clears_all_when_no_session_id() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel, _stops) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert_eq!(body["status"], "ok");
    assert!(
        body["message"]
            .as_str()
            .expect("message is string")
            .contains("all slots cleared")
    );

    cancel.cancel();
}

/// With a valid session ID the handler should return 200 and report that a
/// session was cleared (the message differs from the "all slots" path).
#[tokio::test]
async fn cache_clear_populates_per_session_cleared() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel, _stops) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .header("x-gglib-session-id", "test-session-abc")
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert_eq!(body["status"], "ok");
    // The handler returns "session cleared" when a session ID is provided,
    // distinguishing it from the "all slots cleared" path.
    assert!(
        body["message"]
            .as_str()
            .expect("message is string")
            .contains("session cleared")
    );

    cancel.cancel();
}

/// When cache is enabled but no slot directory was configured, the handler
/// should return 500 Internal Server Error.
#[tokio::test]
async fn cache_clear_returns_500_when_slot_dir_missing() {
    let (base_url, cancel, _stops) = spawn_proxy(true, None).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert!(
        body["error"]
            .as_str()
            .expect("error is string")
            .contains("slot_dir not configured")
    );

    cancel.cancel();
}
