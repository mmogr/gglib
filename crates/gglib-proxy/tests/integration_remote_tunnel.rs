//! The seam nothing else tests: a real gglib proxy, behind a real modelpipe
//! tunnel, reached through a real connect listener.
//!
//! Every other remote test in this crate *synthesises* the tunnel. It sets
//! `via` and `x-modelpipe-peer` by hand against a `StubGateway`, which proves
//! what the proxy does with a request that claims to have come through a
//! tunnel — not what happens to one that actually did. modelpipe's own suite
//! has the opposite gap: it pairs two live endpoints, but against a
//! `MockBackend`, so it proves the pipe and not what is behind it.
//!
//! The two halves had never met. No test in either repository stood up a
//! gglib proxy and reached it through a pipe, which meant the least-tested
//! thing in the whole feature was the feature.
//!
//! ## Why this touches no network
//!
//! Both endpoints run in this process and pair with a real ticket, with
//! `discovery` and `port_mapping` off on both sides — the configuration
//! modelpipe's own
//! `a_pairing_still_forms_with_discovery_and_port_mapping_off` establishes as
//! working. With discovery off the ticket carries only the addresses it was
//! minted with, and those are local, so nothing here reaches n0's discovery
//! service or a relay. A runner with no outbound network runs this suite.
//!
//! ## What it does not prove
//!
//! Nothing about two machines. Both endpoints share a loopback interface, so
//! the hole punch is trivial and the relay is never exercised. That
//! measurement is ADR 0012's two-machine reading, and this is not a
//! substitute for it — it is the part that *can* run on every commit.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort, RemoteGatewayPort};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use modelpipe::{ConnectOptions, ServeOptions, TokenPolicy};
use reqwest::{Client, StatusCode};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};
use fixtures::remote::StubGateway;

/// The one credential, checked at both doors — ADR 0012 decision 2. The same
/// string is the proxy's bearer key and the tunnel edge's supplied token, and
/// that identity is the property this file exists to exercise end to end.
const KEY: &str = "sk-zzq-one-key-two-doors";
const CODE: &str = "483920";

/// Spawn the real proxy on a loopback port, demanding `KEY`.
async fn spawn_proxy() -> (String, CancellationToken) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);
    let gateway = Arc::new(StubGateway::new(CODE, KEY, false));
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(KEY.to_owned()),
        "127.0.0.1",
        vec![],
    )
    .with_remote(Some(gateway as Arc<dyn RemoteGatewayPort>));

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

    tokio::time::sleep(Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel)
}

/// Put a tunnel in front of the proxy and dial it, returning the loopback
/// base URL the far side is now reachable at.
///
/// This is `gglib remote enable` followed by `gglib remote connect`, with the
/// daemon and the CLI taken out of the middle.
async fn tunnel_to(proxy_url: &str) -> (modelpipe::ServeHandle, modelpipe::ConnectHandle, String) {
    let mut serve_opts = ServeOptions::default();
    // The proxy's own key, handed to the tunnel edge. One credential, two
    // doors: the edge rejects a bad bearer before a byte reaches gglib, and
    // gglib's own `bearer_guard` rejects it again behind that.
    serve_opts.auth = TokenPolicy::Supplied(KEY.to_owned());
    serve_opts.discovery = false;
    serve_opts.port_mapping = false;

    let serving = modelpipe::serve(proxy_url, serve_opts)
        .await
        .expect("the tunnel binds in front of the proxy");
    let ticket = serving.ticket();

    let mut connect_opts = ConnectOptions::default();
    connect_opts.discovery = false;
    connect_opts.port_mapping = false;

    let connected = modelpipe::connect(&ticket, connect_opts)
        .await
        .expect("the connect side binds");

    let base = connected.base_url();
    (serving, connected, base)
}

/// Wait for the pipe to actually carry traffic.
///
/// `connect` returns once the local port is bound, not once the far side is
/// reached, so a request sent immediately can beat the connection into
/// existence. Polling a real request rather than the status: what this suite
/// is about is whether a request arrives, and a status that says `Direct` is
/// one layer short of that claim.
async fn get(url: &str, key: Option<&str>) -> reqwest::Response {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let mut request = Client::new().get(url);
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        match request.timeout(Duration::from_secs(5)).send().await {
            Ok(response) => return response,
            Err(error) if std::time::Instant::now() < deadline => {
                // The pipe is still forming. Not a flake being papered over:
                // the contract says `connect` returns before the far side is
                // reached, so this window is documented behaviour.
                let _ = error;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => panic!("the pipe never carried a request: {error}"),
        }
    }
}

/// The claim the whole feature rests on: a request sent to a loopback port on
/// *this* side comes out of a gglib proxy on the far side, and comes back.
#[tokio::test]
async fn a_request_through_the_tunnel_reaches_the_proxy_and_is_answered() {
    let (proxy_url, cancel) = spawn_proxy().await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    let response = get(&format!("{base}/models"), Some(KEY)).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a bearer-carrying request through the tunnel reaches the proxy"
    );

    // The catalog is empty, so the interesting part is the shape rather than
    // the contents: this is gglib's own `/v1/models` answering, not the
    // tunnel inventing a reply.
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["object"], "list", "gglib's own models payload: {body}");

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}

/// The other half of decision 2: the tunnel edge refuses a bad bearer before
/// a byte reaches gglib. The naive embedding this ADR exists to prevent would
/// answer this request, because the proxy sees a loopback bind and a loopback
/// `Host` and trusts both.
#[tokio::test]
async fn a_request_through_the_tunnel_without_the_key_is_refused() {
    let (proxy_url, cancel) = spawn_proxy().await;
    let (serving, connected, base) = tunnel_to(&proxy_url).await;

    // Warm the pipe with a good request first, so a refusal below cannot be
    // a connection that had not formed yet wearing a 401's clothes.
    assert_eq!(
        get(&format!("{base}/models"), Some(KEY)).await.status(),
        StatusCode::OK
    );

    let refused = get(&format!("{base}/models"), None).await;
    assert_eq!(
        refused.status(),
        StatusCode::UNAUTHORIZED,
        "reaching loopback through a tunnel is not being on this machine"
    );

    let wrong = get(&format!("{base}/models"), Some("sk-not-the-key")).await;
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);

    connected.shutdown().await;
    serving.shutdown().await;
    cancel.cancel();
}
