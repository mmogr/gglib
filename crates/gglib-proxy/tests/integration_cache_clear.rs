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

use gglib_core::ports::ModelCatalogPort;
use gglib_core::ports::ModelRuntimePort;

// ─── Proxy harness ─────────────────────────────────────────────────────────

/// Spawn the real `gglib_proxy::serve` with the given cache settings.
/// Returns `(proxy_base_url, cancel_token)`.
async fn spawn_proxy(
    cache_enabled: bool,
    slot_dir: Option<std::path::PathBuf>,
) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(fixtures::common::NoopRuntime);
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
            cache_enabled,
            slot_dir,
        )
        .await
        .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

// ─── Tests ──────────────────────────────────────────────────────────────

/// When cache is disabled, the handler should return 200 with a "skipped"
/// status — it's an idempotent no-op rather than an error.
#[tokio::test]
async fn cache_clear_returns_skipped_when_cache_disabled() {
    let (base_url, cancel) = spawn_proxy(false, None).await;

    let resp = Client::new()
        .post(format!("{base_url}/v1/proxy/cache/clear"))
        .send()
        .await
        .expect("request should succeed");

    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json response");
    assert_eq!(body["status"], "skipped");
    assert!(
        body["message"]
            .as_str()
            .expect("message is string")
            .contains("cache not enabled")
    );

    cancel.cancel();
}

/// A session ID that looks like a path-traversal attempt should be rejected
/// with 400 Bad Request before any filesystem access.
#[tokio::test]
async fn cache_clear_returns_400_on_invalid_session_id() {
    let tmp_dir = tempfile::tempdir().unwrap();
    let (base_url, cancel) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

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
    let (base_url, cancel) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

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
    let (base_url, cancel) = spawn_proxy(true, Some(tmp_dir.path().to_path_buf())).await;

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
    let (base_url, cancel) = spawn_proxy(true, None).await;

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
