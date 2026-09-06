//! Verifies `POST /v1/remote/pair` — trading the one-time code for the key.
//!
//! The tunnel edge admits one request bearing the pairing code; this route is
//! where it ends up. Four things make it safe to have a route that hands out
//! the key, and each is pinned here:
//!
//! * it needs no bearer token, because it cannot demand the credential it
//!   exists to hand out;
//! * it grants exactly once, and every refusal — wrong, spent, burned, no
//!   body, no tunnel — is the same flat 401;
//! * three wrong codes burn the pairing, so the code's twenty bits are not
//!   guessable inside its window;
//! * it is still behind the Host guard, like everything else on this port.
//!
//! Uses the real `gglib_proxy::serve` with a stub for the tunnel's owner.

use std::sync::Arc;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort, RemoteGatewayPort};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use reqwest::{Client, StatusCode};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};
use fixtures::remote::StubGateway;

const CODE: &str = "483920";
const KEY: &str = "sk-zzq-the-real-key";
const TOKEN: &str = "sk-zzq-proxy-token";

/// Spawn the real `gglib_proxy::serve` under `access`.
async fn spawn_proxy(access: ProxyAccessConfig) -> (String, u16, CancellationToken) {
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
    (format!("http://{addr}"), addr.port(), cancel)
}

/// A proxy with a token and a tunnel owner that will accept `CODE` once.
async fn paired_proxy() -> (String, u16, CancellationToken, Arc<StubGateway>) {
    let gateway = Arc::new(StubGateway::new(CODE, KEY, false));
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(TOKEN.to_owned()),
        "127.0.0.1",
        vec![],
    )
    .with_remote(Some(Arc::clone(&gateway) as Arc<dyn RemoteGatewayPort>));
    let (base, port, cancel) = spawn_proxy(access).await;
    (base, port, cancel, gateway)
}

async fn pair(base: &str, body: serde_json::Value) -> reqwest::Response {
    Client::new()
        .post(format!("{base}/v1/remote/pair"))
        .header("via", "1.1 modelpipe")
        .header("x-modelpipe-peer", "3ca82708b995")
        .json(&body)
        .send()
        .await
        .unwrap()
}

/// The whole point: no bearer, the right code, the key comes back — once.
#[tokio::test]
async fn the_right_code_is_traded_for_the_key_exactly_once() {
    let (base, _, cancel, gateway) = paired_proxy().await;

    let res = pair(&base, serde_json::json!({ "code": CODE })).await;
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["api_key"], KEY);
    assert_eq!(
        gateway.paired_peer.lock().unwrap().as_deref(),
        Some("3ca82708b995"),
        "the owner is told which device paired"
    );

    let again = pair(&base, serde_json::json!({ "code": CODE })).await;
    assert_eq!(again.status(), StatusCode::UNAUTHORIZED, "spent");
    let body: serde_json::Value = again.json().await.unwrap();
    assert_eq!(body["error"]["code"], "invalid_pairing_code");

    cancel.cancel();
}

/// Every way of being wrong is the same refusal, and the third wrong code
/// burns the pairing so the right one is dead too.
#[tokio::test]
async fn three_wrong_codes_burn_the_pairing_and_every_refusal_looks_alike() {
    let (base, _, cancel, _) = paired_proxy().await;

    let mut bodies = Vec::new();
    for wrong in ["000000", "483921", "48392"] {
        let res = pair(&base, serde_json::json!({ "code": wrong })).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{wrong}");
        bodies.push(res.text().await.unwrap());
    }
    assert!(
        bodies.windows(2).all(|w| w[0] == w[1]),
        "the refusals must be indistinguishable: {bodies:?}"
    );

    let right = pair(&base, serde_json::json!({ "code": CODE })).await;
    assert_eq!(
        right.status(),
        StatusCode::UNAUTHORIZED,
        "burned: the right code is dead after three misses"
    );

    cancel.cancel();
}

/// A body that carries no code, or does not parse, is a refusal and not a
/// 400 that tells the caller how to fix its request.
#[tokio::test]
async fn a_missing_or_malformed_body_is_the_same_flat_refusal() {
    let (base, _, cancel, _) = paired_proxy().await;

    for body in [
        serde_json::json!({}),
        serde_json::json!({ "code": "" }),
        serde_json::json!({ "code": "   " }),
        serde_json::json!({ "pin": CODE }),
    ] {
        let res = pair(&base, body.clone()).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{body}");
    }
    let raw = Client::new()
        .post(format!("{base}/v1/remote/pair"))
        .header("content-type", "application/json")
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(raw.status(), StatusCode::UNAUTHORIZED);

    let still_valid = pair(&base, serde_json::json!({ "code": CODE })).await;
    assert_eq!(
        still_valid.status(),
        StatusCode::OK,
        "malformed bodies are not attempts and do not burn the code"
    );

    cancel.cancel();
}

/// A proxy nobody attached a tunnel to has no code to redeem, and says so
/// exactly as it would for a wrong one.
#[tokio::test]
async fn a_proxy_with_no_tunnel_owner_refuses_every_code() {
    let (base, _, cancel) = spawn_proxy(ProxyAccessConfig::default()).await;
    let res = pair(&base, serde_json::json!({ "code": CODE })).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    cancel.cancel();
}

/// The route needs no bearer — that is its whole reason to exist — but the
/// Host guard still covers it.
#[tokio::test]
async fn the_route_is_open_to_the_bearer_guard_and_closed_to_the_host_guard() {
    let (base, port, cancel, _) = paired_proxy().await;

    // No `Authorization` was sent by `pair` above, and the code was accepted;
    // this pins that a *wrong* bearer does not change the outcome either.
    let wrong_bearer = Client::new()
        .post(format!("{base}/v1/remote/pair"))
        .bearer_auth("not-the-token")
        .json(&serde_json::json!({ "code": "000000" }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        wrong_bearer.status(),
        StatusCode::UNAUTHORIZED,
        "a wrong code, not a wrong bearer, is what this 401 is about"
    );
    let body: serde_json::Value = wrong_bearer.json().await.unwrap();
    assert_eq!(body["error"]["code"], "invalid_pairing_code");

    let rebound = Client::new()
        .post(format!("http://127.0.0.1:{port}/v1/remote/pair"))
        .header("host", "evil.example")
        .json(&serde_json::json!({ "code": CODE }))
        .send()
        .await
        .unwrap();
    assert_eq!(rebound.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = rebound.json().await.unwrap();
    assert_eq!(body["error"]["code"], "host_not_allowed");

    cancel.cancel();
}
