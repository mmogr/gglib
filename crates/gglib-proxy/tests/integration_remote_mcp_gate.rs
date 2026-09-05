//! Verifies the `/mcp` gate for tunnelled requests, and the tunnel marker
//! the gate and the status surface read.
//!
//! `invoke_tool` starts the MCP servers configured on the serving machine, so
//! a request that arrived through the tunnel may not reach `/mcp` unless the
//! tunnel's owner said so — a leaked token with a shell server configured is
//! remote code execution. The marker that says "tunnelled" is set by the
//! tunnel edge and can also be forged by a local client; these tests pin that
//! forging it only ever buys a refusal.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort, RemoteGatewayPort};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use reqwest::{Client, StatusCode};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};
use fixtures::remote::StubGateway;

const TOKEN: &str = "sk-zzq-proxy-token";

async fn spawn_proxy(access: ProxyAccessConfig) -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            Some(4096),
            true,
            runtime,
            catalog,
            make_mcp_service(),
            cancel_clone,
            None,
            Arc::new(MockSettingsRepo),
            None,
            None,
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            Arc::new(gglib_core::domain::defects::ModelDefectLedger::new()),
            &access,
        )
        .await
        .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

/// A proxy with a token and a tunnel owner whose `/mcp` grant is `allow`.
async fn tunnelled_proxy(allow_mcp: bool) -> (String, CancellationToken, Arc<StubGateway>) {
    let gateway = Arc::new(StubGateway::new("000000", "unused", allow_mcp));
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(TOKEN.to_owned()),
        "127.0.0.1",
        vec![],
    )
    .with_remote(Some(Arc::clone(&gateway) as Arc<dyn RemoteGatewayPort>));
    let (base, cancel) = spawn_proxy(access).await;
    (base, cancel, gateway)
}

/// An MCP `initialize`, authenticated, optionally carrying the edge's markers.
async fn mcp_initialize(base: &str, tunnelled: bool) -> reqwest::Response {
    let mut req = Client::new()
        .post(format!("{base}/mcp"))
        .bearer_auth(TOKEN)
        .header("accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" }
            }
        }));
    if tunnelled {
        req = req
            .header("via", "1.1 modelpipe")
            .header("x-modelpipe-peer", "3ca82708b995");
    }
    req.send().await.unwrap()
}

/// The default: a tunnelled request to `/mcp` is refused even with the key.
#[tokio::test]
async fn a_tunnelled_request_to_mcp_is_refused_by_default() {
    let (base, cancel, _) = tunnelled_proxy(false).await;

    let res = mcp_initialize(&base, true).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"]["code"], "mcp_not_allowed_over_tunnel");

    cancel.cancel();
}

/// The same request without the marker is an ordinary local request and
/// reaches the gateway.
#[tokio::test]
async fn a_local_request_to_mcp_is_not_gated() {
    let (base, cancel, _) = tunnelled_proxy(false).await;

    let res = mcp_initialize(&base, false).await;
    assert_ne!(res.status(), StatusCode::FORBIDDEN);
    assert!(
        res.headers().contains_key("mcp-session-id"),
        "initialize reached the gateway: {}",
        res.status()
    );

    cancel.cancel();
}

/// `--allow-mcp` opens the gate.
#[tokio::test]
async fn an_allowed_tunnel_reaches_mcp() {
    let (base, cancel, _) = tunnelled_proxy(true).await;

    let res = mcp_initialize(&base, true).await;
    assert_ne!(res.status(), StatusCode::FORBIDDEN);
    assert!(res.headers().contains_key("mcp-session-id"));

    cancel.cancel();
}

/// The bearer guard runs first: an unauthenticated tunnelled request is a
/// 401, and the gate never learns it existed.
#[tokio::test]
async fn the_bearer_guard_still_comes_first() {
    let (base, cancel, _) = tunnelled_proxy(false).await;

    let res = Client::new()
        .post(format!("{base}/mcp"))
        .header("via", "1.1 modelpipe")
        .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    cancel.cancel();
}

/// A proxy with no tunnel owner treats a marker as what it is there — a
/// forgery — and refuses `/mcp` for it. Forging the marker buys a refusal.
#[tokio::test]
async fn a_forged_marker_on_a_proxy_with_no_tunnel_only_denies_itself() {
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(TOKEN.to_owned()),
        "127.0.0.1",
        vec![],
    );
    let (base, cancel) = spawn_proxy(access).await;

    let forged = mcp_initialize(&base, true).await;
    assert_eq!(forged.status(), StatusCode::FORBIDDEN);
    let honest = mcp_initialize(&base, false).await;
    assert_ne!(honest.status(), StatusCode::FORBIDDEN);

    cancel.cancel();
}

/// Tunnelled requests are reported to the owner with their peer; local ones
/// are not.
#[tokio::test]
async fn tunnelled_requests_are_counted_and_local_ones_are_not() {
    let (base, cancel, gateway) = tunnelled_proxy(false).await;

    let local = Client::new()
        .get(format!("{base}/v1/models"))
        .bearer_auth(TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(local.status(), StatusCode::OK);
    assert_eq!(gateway.tunnelled.load(Ordering::SeqCst), 0);

    let remote = Client::new()
        .get(format!("{base}/v1/models"))
        .bearer_auth(TOKEN)
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", "3ca82708b995")
        .send()
        .await
        .unwrap();
    assert_eq!(
        remote.status(),
        StatusCode::OK,
        "inference routes stay open"
    );
    assert_eq!(gateway.tunnelled.load(Ordering::SeqCst), 1);
    assert_eq!(
        gateway.last_peer.lock().unwrap().as_deref(),
        Some("3ca82708b995")
    );

    // Refused by the Host guard before the marker runs: not counted.
    let rebound = Client::new()
        .get(format!("{base}/v1/models"))
        .header("host", "evil.example")
        .header("via", "1.1 modelpipe")
        .send()
        .await
        .unwrap();
    assert_eq!(rebound.status(), StatusCode::FORBIDDEN);
    assert_eq!(gateway.tunnelled.load(Ordering::SeqCst), 1);

    cancel.cancel();
}
