//! A real gglib proxy behind a real modelpipe tunnel, in one process.
//!
//! Shared by the two suites that exercise the seam end to end rather than
//! synthesising it: `integration_remote_tunnel.rs`, which is about a request
//! crossing the pipe at all, and `integration_remote_devices.rs`, which is
//! about *which* credentials get to cross it.
//!
//! ## Why this touches no network
//!
//! Both endpoints run in this process and pair with a real ticket, with
//! `discovery` and `port_mapping` off on both sides — the configuration
//! modelpipe's own
//! `a_pairing_still_forms_with_discovery_and_port_mapping_off` establishes as
//! working. With discovery off the ticket carries only the addresses it was
//! minted with, and those are local, so nothing here reaches n0's discovery
//! service or a relay. A runner with no outbound network runs these suites.
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

use super::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};
use super::remote::StubGateway;

/// The credential the *proxy* demands, and the one the tunnel edge presents
/// upstream on every admitted request. It never leaves this machine: a device
/// holds a key of its own, and `ServeOptions::backend_auth` replaces it here.
pub(crate) const PROXY_KEY: &str = "sk-zzq-the-backend-key";

/// One paired device: the name the edge holds its key under, and the key.
pub(crate) const DEVICE: &str = "dev-0a1b2c3d";
pub(crate) const DEVICE_KEY: &str = "sk-zzq-the-laptops-key";

/// The code the stub gateway will redeem, for the pairing-route tests.
pub(crate) const CODE: &str = "483920";

/// Spawn the real proxy on a loopback port, demanding [`PROXY_KEY`].
pub(crate) async fn spawn_proxy() -> (String, CancellationToken, Arc<StubGateway>) {
    spawn_proxy_demanding(PROXY_KEY).await
}

/// The same, for a proxy whose bearer is *not* what the tunnel was armed
/// with — which is the state a rotation passes through.
pub(crate) async fn spawn_proxy_demanding(
    key: &str,
) -> (String, CancellationToken, Arc<StubGateway>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);
    let gateway = Arc::new(StubGateway::new(CODE, DEVICE_KEY, false));
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(key.to_owned()),
        "127.0.0.1",
        vec![],
    )
    .with_remote(Some(Arc::clone(&gateway) as Arc<dyn RemoteGatewayPort>));

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
    (format!("http://{addr}"), cancel, gateway)
}

/// Put a tunnel in front of the proxy and dial it, returning the loopback base
/// URL the far side is now reachable at.
///
/// This is `gglib remote enable --invite` on one machine and a redemption on
/// the other, with the daemon and the CLI taken out of the middle:
/// the listener runs `TokenPolicy::Named` and holds exactly one device's key,
/// and [`PROXY_KEY`] is the backend credential the edge swaps in.
pub(crate) async fn tunnel_to(
    proxy_url: &str,
) -> (modelpipe::ServeHandle, modelpipe::ConnectHandle, String) {
    let mut serve_opts = ServeOptions::default();
    // Named, not Supplied. Nothing admits but a key this machine issued to a
    // named device, or a live one-time grant — and only the first of those
    // makes the edge write `X-Modelpipe-Device`, which is what gglib's device
    // gate reads.
    serve_opts.auth = TokenPolicy::Named;
    serve_opts.backend_auth = Some(PROXY_KEY.to_owned());
    serve_opts.discovery = false;
    serve_opts.port_mapping = false;

    let serving = modelpipe::serve(proxy_url, serve_opts)
        .await
        .expect("the tunnel binds in front of the proxy");
    serving
        .add_token(DEVICE, DEVICE_KEY.to_owned())
        .expect("the listener holds the paired device's key");
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

/// modelpipe's one retryable 502, and the only one.
///
/// The connect side writes this while there is no peer to forward to. The
/// other two — `bad_gateway` and `backend_unreachable` — are written by the
/// *serving* side about a model server that is stopped or wedged behind a
/// pipe that is working, and no amount of waiting fixes those.
///
/// gglib's product code draws exactly this line, for exactly this reason: see
/// `code_is_retryable` in
/// `gglib-runtime/src/ports_impl/llm_completion/retry/classify.rs`. It is
/// restated here rather than shared because `gglib-proxy` does not depend on
/// `gglib-runtime` and must not start in order to spell one string.
const TUNNEL_UNAVAILABLE: &str = "tunnel_unavailable";

/// What a request came back with. Carries the body rather than the
/// `Response`, because reading the body is how one 502 is told from another
/// and reading it consumes the response.
pub(crate) struct Answer {
    pub(crate) status: StatusCode,
    pub(crate) body: String,
}

impl Answer {
    pub(crate) fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {}", self.body))
    }
}

/// Wait for the pipe to actually carry traffic.
///
/// `connect` returns once the local port is bound, not once the far side is
/// reached, so a request sent immediately can beat the connection into
/// existence. Polling a real request rather than the status: what these
/// suites are about is whether a request arrives, and a status that says
/// `Direct` is one layer short of that claim.
///
/// **That window has two exits, and this used to cover only one.** A request
/// issued before the peer is reached can fail at the transport — which
/// arrives as `Err` and was retried — or be answered `502 tunnel_unavailable`
/// by the connecting side's own edge, which is a perfectly well-formed HTTP
/// response and so arrived as `Ok` and was handed back as the final answer.
/// On one Mac it took the second exit every time, and both tests failed in
/// under a fifth of a second with the deadline never engaging. The design was
/// right; the predicate was one status code short.
///
/// `Client::new()` stays inside the loop deliberately. On Linux it eagerly
/// reads the system trust store while macOS does not, and that difference is
/// the leading explanation for why this suite has been green in CI and red
/// there. Hoisting it is a change to the thing under measurement.
pub(crate) async fn get(url: &str, key: Option<&str>) -> Answer {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let mut request = Client::new().get(url);
        if let Some(key) = key {
            request = request.bearer_auth(key);
        }
        match request.timeout(Duration::from_secs(5)).send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if status != StatusCode::BAD_GATEWAY || !body.contains(TUNNEL_UNAVAILABLE) {
                    return Answer { status, body };
                }
                // The pipe is still forming. Not a flake being papered over:
                // the contract says `connect` returns before the far side is
                // reached, so this window is documented behaviour.
                assert!(
                    std::time::Instant::now() < deadline,
                    "the edge answered `{TUNNEL_UNAVAILABLE}` for the whole deadline, \
                     so the far side was never reached: {body}"
                );
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) if std::time::Instant::now() < deadline => {
                // The other exit from the same window: the listener is bound
                // but nothing is behind it yet.
                let _ = error;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => panic!("the pipe never carried a request: {error}"),
        }
    }
}
