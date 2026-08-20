//! Verifies the proxy's LocalOnly CORS layer.
//!
//! The proxy binds to 127.0.0.1 and accepts only local origins:
//! `localhost`, `127.0.0.1`, `::1`, `tauri.localhost`, and Tauri custom
//! schemes (`tauri://localhost`, `asset://localhost`).
//! Non-local origins are rejected.
//!
//! This ensures the Tauri GUI's webview (origin `tauri://localhost` /
//! `http://tauri.localhost` on Windows) can call this proxy's endpoints —
//! including opening an `EventSource` connection to `GET /v1/proxy/status/stream`
//! — without the browser blocking the request as cross-origin.
//!
//! Uses the real `gglib_proxy::serve` (not a hand-rolled router), sharing its
//! mock ports with the other integration tests via `tests/fixtures` rather
//! than duplicating them.

use std::sync::Arc;

use reqwest::Client;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};

// ─── Proxy harness ─────────────────────────────────────────────────────────

/// Spawn the real `gglib_proxy::serve` with no upstream configured (not
/// needed — these tests only exercise `/v1/proxy/status`, which doesn't
/// touch the runtime/catalog ports). Returns `(proxy_base_url, cancel)`.
async fn spawn_proxy() -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            make_mcp_service(),
            cancel_clone,
            Arc::new(MockSettingsRepo),
            None, // inference_override
            None, // default_profile
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            std::sync::Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            std::sync::Arc::new(gglib_core::domain::defects::ModelDefectLedger::new()),
            &gglib_core::ProxyAccessConfig::default(),
        )
        .await
        .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

// ─── Tests ──────────────────────────────────────────────────────────────

/// A plain GET from a Tauri-webview origin must come back with
/// `access-control-allow-origin` set — this is exactly what an
/// `EventSource` connection to `/v1/proxy/status/stream` needs, since
/// `EventSource` performs a simple GET (no preflight) but the browser still
/// enforces CORS on the response.
#[tokio::test]
async fn get_request_from_tauri_origin_receives_cors_header() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .get(format!("{base_url}/v1/proxy/status"))
        .header("Origin", "tauri://localhost")
        .send()
        .await
        .expect("request should succeed");

    assert!(resp.status().is_success());
    let allow_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .expect("missing access-control-allow-origin header")
        .to_str()
        .unwrap();
    // LocalOnly reflects the requesting origin back (not `*`), which is
    // valid for a non-credentialed request from a local origin like
    // `tauri://localhost`.
    assert_eq!(allow_origin, "tauri://localhost");

    cancel.cancel();
}

/// A CORS preflight (`OPTIONS` with `Access-Control-Request-Method`) against
/// the SSE endpoint must succeed and carry the CORS response headers, which
/// is what a browser sends before allowing the actual `EventSource`/fetch
/// call through.
#[tokio::test]
async fn preflight_request_to_sse_endpoint_is_allowed() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .request(
            reqwest::Method::OPTIONS,
            format!("{base_url}/v1/proxy/status/stream"),
        )
        .header("Origin", "tauri://localhost")
        .header("Access-Control-Request-Method", "GET")
        .send()
        .await
        .expect("preflight request should succeed");

    assert!(
        resp.status().is_success(),
        "preflight should not be rejected, got {}",
        resp.status()
    );
    assert!(
        resp.headers().contains_key("access-control-allow-origin"),
        "preflight response missing access-control-allow-origin"
    );
    assert!(
        resp.headers().contains_key("access-control-allow-methods"),
        "preflight response missing access-control-allow-methods"
    );

    cancel.cancel();
}

/// A request from a plain `http://localhost:5173` (Vite dev server) origin
/// works identically — the LocalOnly layer accepts any localhost origin
/// and reflects it back.
#[tokio::test]
async fn get_request_from_vite_dev_origin_receives_cors_header() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .get(format!("{base_url}/v1/proxy/status"))
        .header("Origin", "http://localhost:5173")
        .send()
        .await
        .expect("request should succeed");

    assert!(resp.status().is_success());
    let allow_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .expect("missing access-control-allow-origin header")
        .to_str()
        .unwrap();
    assert_eq!(allow_origin, "http://localhost:5173");

    cancel.cancel();
}

/// A request from `http://tauri.localhost` (Tauri dev server on Windows)
/// is accepted and reflected back by the LocalOnly CORS policy.
#[tokio::test]
async fn get_request_from_tauri_localhost_origin_receives_cors_header() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .get(format!("{base_url}/v1/proxy/status"))
        .header("Origin", "http://tauri.localhost")
        .send()
        .await
        .expect("request should succeed");

    assert!(resp.status().is_success());
    let allow_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .expect("missing access-control-allow-origin header")
        .to_str()
        .unwrap();
    assert_eq!(allow_origin, "http://tauri.localhost");

    cancel.cancel();
}

/// A request from an external (non-local) origin must NOT receive the
/// `access-control-allow-origin` header — the tower-http CORS layer
/// silently omits it rather than returning 403; the browser enforces
/// the block client-side.
#[tokio::test]
async fn get_request_from_external_origin_is_rejected() {
    let (base_url, cancel) = spawn_proxy().await;

    let resp = Client::new()
        .get(format!("{base_url}/v1/proxy/status"))
        .header("Origin", "http://evil.com")
        .send()
        .await
        .expect("request should succeed");

    // The CORS layer does NOT return 403 for rejected origins — it returns
    // 200 (or whatever the handler returns) but omits the CORS header.
    assert!(resp.status().is_success());
    let allow_origin = resp.headers().get("access-control-allow-origin");
    assert!(
        allow_origin.is_none(),
        "Remote origin should be rejected (no access-control-allow-origin header), got: {:?}",
        allow_origin
    );

    cancel.cancel();
}
