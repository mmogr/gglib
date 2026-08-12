//! Verifies the proxy shuts down promptly while a dashboard stream is open.
//!
//! `serve` uses `axum::serve(...).with_graceful_shutdown(...)`, which waits for
//! every in-flight connection to close before returning. `GET
//! /v1/proxy/status/stream` is an SSE response, and an SSE response does not
//! end on its own — so unless the stream itself is bounded by the shutdown
//! token, one subscriber is enough to stop `serve` ever returning.
//!
//! That is not hypothetical: the desktop app's tray panel holds a dashboard
//! stream open the entire time the proxy is running, which made every stop hit
//! `ProxySupervisor::stop`'s 5s timeout and get aborted, surfacing to the user
//! as "Proxy stop timed out; task aborted".
//!
//! Uses the real `gglib_proxy::serve` and keeps the join handle, since the
//! assertion here is specifically about `serve` returning.

use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};

mod fixtures;
use fixtures::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};

/// The supervisor aborts the task after this long, turning a slow shutdown
/// into a user-visible error. Assert well inside it so the test fails on a
/// regression rather than on a slow machine.
const SUPERVISOR_ABORT_AFTER: Duration = Duration::from_secs(5);

/// Spawn the real `serve`, returning its join handle so shutdown can be awaited.
async fn spawn_proxy() -> (String, CancellationToken, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let runtime: Arc<dyn ModelRuntimePort> = Arc::new(NoopRuntime);
    let catalog: Arc<dyn ModelCatalogPort> = Arc::new(EmptyCatalog);

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();
    let handle = tokio::spawn(async move {
        gglib_proxy::serve(
            listener,
            4096,
            runtime,
            catalog,
            make_mcp_service(),
            cancel_clone,
            Arc::new(MockSettingsRepo),
            None, // inference_override
            false,
            None,
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            Arc::new(gglib_core::domain::defects::ModelDefectLedger::new()),
            &gglib_core::ProxyAccessConfig::default(),
        )
        .await
        .ok();
    });

    tokio::time::sleep(Duration::from_millis(30)).await;
    (format!("http://{addr}"), cancel, handle)
}

/// The regression: with a subscriber attached, `serve` must still return.
///
/// Before the stream was bounded this hung indefinitely, and only the
/// supervisor's abort ended it — which is what produced the timeout error.
#[tokio::test]
async fn shutdown_completes_while_a_dashboard_stream_is_open() {
    let (base, cancel, handle) = spawn_proxy().await;

    // Hold a real SSE connection open, and read the hydration frame so the
    // stream is definitely established before shutdown is requested.
    let response = reqwest::Client::new()
        .get(format!("{base}/v1/proxy/status/stream"))
        .send()
        .await
        .expect("stream connects");
    assert!(response.status().is_success());
    let _stream = response.bytes_stream();

    cancel.cancel();

    let shut_down = tokio::time::timeout(SUPERVISOR_ABORT_AFTER, handle).await;

    assert!(
        shut_down.is_ok(),
        "serve did not return within {SUPERVISOR_ABORT_AFTER:?} with a dashboard \
         subscriber attached - the supervisor would abort it and report a timeout"
    );
}

/// The same shutdown with nobody attached, so a regression in the bounded path
/// cannot hide behind an unrelated hang.
#[tokio::test]
async fn shutdown_completes_with_no_subscribers() {
    let (_base, cancel, handle) = spawn_proxy().await;

    cancel.cancel();

    assert!(
        tokio::time::timeout(SUPERVISOR_ABORT_AFTER, handle)
            .await
            .is_ok(),
        "serve did not return with no subscribers attached"
    );
}

/// Several concurrent subscribers must not each add a way to wedge shutdown:
/// the desktop app can genuinely have the tray panel, the dashboard modal and
/// a CLI dashboard attached at once.
#[tokio::test]
async fn shutdown_completes_with_several_streams_open() {
    let (base, cancel, handle) = spawn_proxy().await;

    let client = reqwest::Client::new();
    let mut streams = Vec::new();
    for _ in 0..3 {
        let response = client
            .get(format!("{base}/v1/proxy/status/stream"))
            .send()
            .await
            .expect("stream connects");
        assert!(response.status().is_success());
        streams.push(response.bytes_stream());
    }

    cancel.cancel();

    assert!(
        tokio::time::timeout(SUPERVISOR_ABORT_AFTER, handle)
            .await
            .is_ok(),
        "serve did not return with three dashboard subscribers attached"
    );
}
