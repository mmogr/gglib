//! Verifies `POST /v1/proxy/shutdown` — the only way to stop this machine
//! from the other side of a tunnel.
//!
//! The daemon already has `POST /api/daemon/shutdown`, and it is on a port a
//! remote client cannot reach: the tunnel forwards to exactly one backend,
//! this proxy. So the route lives here, inside the bearer-guarded group, and
//! these tests pin the four things that make it safe to expose:
//!
//! * it is behind the token, like everything else in that group;
//! * a request that crossed the tunnel must have come in on a *device* key,
//!   because `backend_auth` leaves the token check unable to refuse one that
//!   did not;
//! * it will not fire without an explicit confirmation, because it is a
//!   one-way door — nothing brings the daemon back but physical access;
//! * it says so rather than pretending when there is no daemon to stop.

use std::sync::Arc;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use reqwest::{Client, StatusCode};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};

/// Spawn the real `gglib_proxy::serve`, optionally under a daemon.
///
/// `daemon` is what a daemon would have handed over; `None` is the embedded
/// case. Returns the base URL and the proxy's own cancel token.
async fn spawn_proxy(
    access: ProxyAccessConfig,
    daemon: Option<CancellationToken>,
) -> (String, CancellationToken) {
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
            daemon,
            Arc::new(MockSettingsRepo),
            None,  // inference_override
            None,  // default_profile
            false, // cache_enabled
            None,  // slot_dir
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

fn with_key(key: &str) -> ProxyAccessConfig {
    ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(key.to_owned()),
        "127.0.0.1",
        vec![],
    )
}

/// The whole point: an authenticated client, one tunnel away, stops the
/// daemon.
#[tokio::test]
async fn a_confirmed_request_with_the_token_cancels_the_daemon() {
    let daemon = CancellationToken::new();
    let (base, cancel) = spawn_proxy(with_key("secret123"), Some(daemon.clone())).await;

    assert!(!daemon.is_cancelled(), "nothing has asked yet");

    let res = Client::new()
        .post(format!("{base}/v1/proxy/shutdown"))
        .bearer_auth("secret123")
        .json(&serde_json::json!({ "confirm": "shutdown" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::ACCEPTED);
    assert!(
        daemon.is_cancelled(),
        "the daemon's token must be cancelled, which is what stops everything"
    );

    cancel.cancel();
}

/// It is in the protected group, so it is the token that gates it — the same
/// credential and the same 401 as inference.
#[tokio::test]
async fn an_unauthenticated_request_cannot_stop_anything() {
    let daemon = CancellationToken::new();
    let (base, cancel) = spawn_proxy(with_key("secret123"), Some(daemon.clone())).await;

    for credential in [None, Some("Bearer wrong")] {
        let mut request = Client::new().post(format!("{base}/v1/proxy/shutdown"));
        if let Some(value) = credential {
            request = request.header(reqwest::header::AUTHORIZATION, value);
        }
        let res = request
            .json(&serde_json::json!({ "confirm": "shutdown" }))
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{credential:?}");
        assert!(!daemon.is_cancelled(), "and nothing was stopped");
    }

    cancel.cancel();
}

/// Holding the key is not the same as meaning it. A retried request, a
/// prefetch, or a shell history entry recalled one line off must not strand
/// the machine.
#[tokio::test]
async fn the_key_alone_is_not_enough_without_the_confirmation() {
    let daemon = CancellationToken::new();
    let (base, cancel) = spawn_proxy(with_key("secret123"), Some(daemon.clone())).await;

    let bodies = [
        serde_json::json!({}),
        serde_json::json!({ "confirm": "" }),
        serde_json::json!({ "confirm": "yes" }),
        serde_json::json!({ "confirm": "SHUTDOWN" }),
    ];
    for body in bodies {
        let res = Client::new()
            .post(format!("{base}/v1/proxy/shutdown"))
            .bearer_auth("secret123")
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{body}");
        assert!(!daemon.is_cancelled(), "{body} must not stop the daemon");
    }

    // A request with no body at all is the same answer, not a panic.
    let res = Client::new()
        .post(format!("{base}/v1/proxy/shutdown"))
        .bearer_auth("secret123")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(!daemon.is_cancelled());

    cancel.cancel();
}

/// An embedded proxy has no daemon behind it. Reporting that is better than
/// answering 202 for a shutdown nothing would carry out.
#[tokio::test]
async fn a_proxy_with_no_daemon_says_so() {
    let (base, cancel) = spawn_proxy(with_key("secret123"), None).await;

    let res = Client::new()
        .post(format!("{base}/v1/proxy/shutdown"))
        .bearer_auth("secret123")
        .json(&serde_json::json!({ "confirm": "shutdown" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CONFLICT);

    cancel.cancel();
}

/// The fourth thing that makes this route safe to expose, added with
/// per-device keys: the request must have come in on a device key.
///
/// `ServeOptions::backend_auth` means the bearer on a tunnelled request is a
/// header modelpipe wrote microseconds earlier, so `bearer_guard` can no
/// longer refuse anything that crossed the tunnel — including the one request
/// a pairing **grant** admits, which the edge cannot scope to a path. The
/// device gate is what refuses that, and this is the route where being wrong
/// costs the most: nothing brings the daemon back but physical access.
///
/// Synthesised rather than piped, deliberately. The real grant is pinned over
/// a live tunnel in `integration_remote_devices.rs`; this one is here because
/// an unrelated change to the router's layer order would quietly reopen it,
/// and this file is where somebody looks when they touch this route.
#[tokio::test]
async fn a_tunnelled_request_with_no_device_cannot_stop_anything() {
    let daemon = CancellationToken::new();
    let (base, cancel) = spawn_proxy(with_key("secret123"), Some(daemon.clone())).await;

    let res = Client::new()
        .post(format!("{base}/v1/proxy/shutdown"))
        .bearer_auth("secret123")
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", "3ca82708b995")
        .json(&serde_json::json!({ "confirm": "shutdown" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"]["code"], "device_not_paired");
    assert!(
        !daemon.is_cancelled(),
        "a request the gate refused must not have stopped anything"
    );

    // Named by a device, the same request goes through — so the refusal above
    // is the missing header and not the marker.
    let named = Client::new()
        .post(format!("{base}/v1/proxy/shutdown"))
        .bearer_auth("secret123")
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", "3ca82708b995")
        .header("x-modelpipe-device", "dev-0a1b2c3d")
        .json(&serde_json::json!({ "confirm": "shutdown" }))
        .send()
        .await
        .unwrap();
    assert_eq!(named.status(), StatusCode::ACCEPTED);
    assert!(daemon.is_cancelled());

    cancel.cancel();
}
