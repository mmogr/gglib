//! Verifies the proxy's two request guards: the optional bearer token and the
//! always-on Host-header allowlist.
//!
//! The endpoint has an MCP gateway attached that executes filesystem tools, so
//! "who may talk to this port" is not a theoretical question. Two independent
//! answers are asserted here:
//!
//! * **Bearer token** — opt-in. Unset must behave exactly as the proxy always
//!   did, because every existing loopback setup depends on that. Set, it must
//!   cover `/v1/*` and `/mcp` while leaving `/health` reachable.
//! * **Host allowlist** — always on. It is what stops a DNS-rebound browser
//!   tab from reaching a loopback-bound proxy, and it holds whether or not a
//!   token is configured.
//!
//! Uses the real `gglib_proxy::serve` (not a hand-rolled router), sharing its
//! mock ports with the other integration tests via `tests/fixtures` rather
//! than duplicating them.

use std::sync::Arc;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use reqwest::{Client, StatusCode};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};

// ─── Proxy harness ─────────────────────────────────────────────────────────

/// Spawn the real `gglib_proxy::serve` under a given access policy.
///
/// No upstream is configured — none of these tests reach a handler that needs
/// one. `/v1/models` is served from the (empty) catalog, and every other
/// assertion is about a request being refused before it gets that far.
///
/// Returns `(proxy_base_url, port, cancel)`. The port is returned separately
/// because the Host-header tests need to construct authorities by hand.
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
            4096,
            runtime,
            catalog,
            make_mcp_service(),
            cancel_clone,
            Arc::new(MockSettingsRepo),
            None,  // inference_override
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
    (format!("http://{addr}"), addr.port(), cancel)
}

/// An access policy with a token, bound loopback.
fn with_key(key: &str) -> ProxyAccessConfig {
    ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(key.to_owned()),
        "127.0.0.1",
        vec![],
    )
}

// ─── No token configured: nothing changes ──────────────────────────────────

/// The regression that matters most. Authentication is opt-in, and every
/// existing local setup runs without it — if this ever fails, the feature has
/// broken more than it protects.
#[tokio::test]
async fn an_unconfigured_proxy_serves_every_route_without_credentials() {
    let (base, _port, cancel) = spawn_proxy(ProxyAccessConfig::default()).await;

    for path in ["/health", "/v1/models", "/v1/proxy/status"] {
        let res = Client::new()
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "{path} should be open when no key is configured"
        );
    }

    cancel.cancel();
}

// ─── Token configured ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_correct_bearer_token_is_accepted() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let res = Client::new()
        .get(format!("{base}/v1/models"))
        .bearer_auth("secret123")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    cancel.cancel();
}

/// Every shape of a wrong credential, including the two that a lenient
/// comparison would wave through: a prefix, and the raw token with no scheme.
#[tokio::test]
async fn every_flavour_of_missing_or_wrong_credential_is_refused() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let cases: Vec<(&str, Option<&str>)> = vec![
        ("no Authorization header at all", None),
        ("a different key", Some("Bearer nope")),
        (
            "the right key under the wrong scheme",
            Some("Basic secret123"),
        ),
        ("the key with no scheme", Some("secret123")),
        ("a prefix of the expected header", Some("Bearer secret")),
        (
            "the expected header plus a suffix",
            Some("Bearer secret1234"),
        ),
        ("an empty header", Some("")),
    ];

    for (description, header) in cases {
        let mut request = Client::new().get(format!("{base}/v1/models"));
        if let Some(value) = header {
            request = request.header(reqwest::header::AUTHORIZATION, value);
        }
        let res = request.send().await.unwrap();
        assert_eq!(
            res.status(),
            StatusCode::UNAUTHORIZED,
            "{description} should be refused"
        );
    }

    cancel.cancel();
}

/// `WWW-Authenticate` is how a client learns *how* to authenticate rather than
/// merely that it failed to.
#[tokio::test]
async fn a_rejection_advertises_the_scheme_and_names_the_problem() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let res = Client::new()
        .get(format!("{base}/v1/models"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        res.headers()
            .get(reqwest::header::WWW_AUTHENTICATE)
            .and_then(|v| v.to_str().ok()),
        Some("Bearer")
    );

    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"]["code"], "invalid_api_key");

    cancel.cancel();
}

/// A supervisor, a container healthcheck or `gglib up`'s bind probe all poll
/// `/health` before they could possibly hold a credential.
#[tokio::test]
async fn health_stays_open_when_a_key_is_configured() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let res = Client::new()
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    cancel.cancel();
}

/// The MCP gateway executes filesystem tools, which makes it the single most
/// important route on this list.
#[tokio::test]
async fn the_mcp_gateway_requires_the_token() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let initialize = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } }
    });

    let res = Client::new()
        .post(format!("{base}/mcp"))
        .json(&initialize)
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    cancel.cancel();
}

/// Embeddings loads and runs a model exactly like chat completions does, so
/// it belongs in the protected group and not beside `/health`. A route added
/// to the wrong group fails silently otherwise.
#[tokio::test]
async fn the_embeddings_route_requires_the_token() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let res = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&serde_json::json!({ "model": "test-model", "input": "hello" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    cancel.cancel();
}

/// The dashboard routes are inside the protected group, which is what forces
/// the GUI and the CLI dashboard to authenticate.
#[tokio::test]
async fn the_dashboard_routes_require_the_token() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    for path in ["/v1/proxy/status", "/v1/proxy/status/stream"] {
        let res = Client::new()
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{path}");
    }

    let res = Client::new()
        .post(format!("{base}/v1/proxy/cache/clear"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

    cancel.cancel();
}

// ─── Host allowlist ────────────────────────────────────────────────────────

/// The rebinding guard proper. A valid token is presented deliberately: the
/// Host check must not be something authentication can buy its way past, since
/// a rebound page could be carrying a token stolen some other way.
#[tokio::test]
async fn a_foreign_host_is_refused_even_with_a_valid_token() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let res = Client::new()
        .get(format!("{base}/v1/models"))
        .header(reqwest::header::HOST, "evil.com")
        .bearer_auth("secret123")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["error"]["code"], "host_not_allowed");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--allowed-host"),
        "the message must name the flag that fixes it: {body}"
    );

    cancel.cancel();
}

/// The guard is on even for an endpoint with no token — that is the whole
/// point of it being independent of authentication.
#[tokio::test]
async fn the_host_guard_applies_to_an_unauthenticated_proxy_and_to_health() {
    let (base, _port, cancel) = spawn_proxy(ProxyAccessConfig::default()).await;

    for path in ["/v1/models", "/health"] {
        let res = Client::new()
            .get(format!("{base}{path}"))
            .header(reqwest::header::HOST, "evil.com")
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{path}");
    }

    cancel.cancel();
}

/// However a client spells loopback, it has to work — clients do not agree on
/// which of these to send.
#[tokio::test]
async fn every_spelling_of_loopback_is_accepted() {
    let (base, port, cancel) = spawn_proxy(ProxyAccessConfig::default()).await;

    for host in [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
        "127.0.0.1".to_string(),
        format!("LOCALHOST:{port}"),
    ] {
        let res = Client::new()
            .get(format!("{base}/v1/models"))
            .header(reqwest::header::HOST, &host)
            .send()
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK, "Host: {host}");
    }

    cancel.cancel();
}

/// The escape hatch for a wildcard bind, which grants nothing by itself.
#[tokio::test]
async fn an_explicitly_allowed_host_is_accepted_under_a_wildcard_bind() {
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        None,
        "0.0.0.0",
        vec!["gglib.lan".to_owned()],
    );
    let (base, port, cancel) = spawn_proxy(access).await;

    let allowed = Client::new()
        .get(format!("{base}/v1/models"))
        .header(reqwest::header::HOST, format!("gglib.lan:{port}"))
        .send()
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);

    // A wildcard bind confers nothing on its own, so a LAN address nobody
    // named is still refused.
    let refused = Client::new()
        .get(format!("{base}/v1/models"))
        .header(reqwest::header::HOST, format!("192.168.1.5:{port}"))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);

    cancel.cancel();
}

// ─── Layer ordering ────────────────────────────────────────────────────────

/// A preflight carries no credentials — it exists to ask whether the real
/// request may carry them. If `CorsLayer` were not outermost this would come
/// back 401 and every browser client would break the moment a key was set.
#[tokio::test]
async fn cors_preflight_is_answered_rather_than_challenged() {
    let (base, _port, cancel) = spawn_proxy(with_key("secret123")).await;

    let res = Client::new()
        .request(
            reqwest::Method::OPTIONS,
            format!("{base}/v1/chat/completions"),
        )
        .header(reqwest::header::ORIGIN, "http://localhost:5173")
        .header("Access-Control-Request-Method", "POST")
        .header(
            "Access-Control-Request-Headers",
            "authorization,content-type",
        )
        .send()
        .await
        .unwrap();

    assert_ne!(
        res.status(),
        StatusCode::UNAUTHORIZED,
        "preflight must not be challenged for the credentials it is asking about"
    );
    assert!(
        res.headers().contains_key("access-control-allow-origin"),
        "preflight should be answered by the CORS layer"
    );

    cancel.cancel();
}
