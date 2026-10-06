//! The one place these tests start the real `gglib_proxy::serve`.
//!
//! [`defaults`] is the proxy a test gets when it says nothing: no upstream, an
//! empty catalog, no key, no cache. A test names what it is about by setting
//! those fields and leaving the rest, `ServeConfig { access, ..defaults().await }`,
//! and [`spawn`] serves the result.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::ProxyAccessConfig;
use gglib_core::cache_metrics::CacheMetricsStore;
use gglib_proxy::slot_eviction::DiskBudget;
use gglib_proxy::{ProxyObservers, ServeConfig, StreamBounds, TEST_STREAM_BOUNDS};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};

/// A proxy that is being served.
pub(crate) struct Spawned {
    /// Where it answers, as `http://127.0.0.1:{port}`.
    pub(crate) base: String,
    /// The port alone, for a test that builds an authority by hand.
    pub(crate) port: u16,
    /// The config's `cancel`: firing it stops the proxy.
    pub(crate) cancel: CancellationToken,
    /// The task `serve` runs in, for a test about `serve` returning.
    pub(crate) handle: JoinHandle<()>,
}

/// What a proxy is served with unless the test says otherwise: a fresh
/// loopback listener and cancel token, a runtime that launches nothing, an
/// empty catalog, and neither a key, a daemon nor a cache.
pub(crate) async fn defaults() -> ServeConfig {
    ServeConfig {
        listener: TcpListener::bind("127.0.0.1:0").await.unwrap(),
        default_ctx: Some(4096),
        // Device memory readable: no suite here is about the fit.
        device_memory_readable: true,
        runtime_port: Arc::new(NoopRuntime),
        catalog_port: Arc::new(EmptyCatalog),
        mcp: make_mcp_service(),
        cancel: CancellationToken::new(),
        daemon_cancel: None,
        settings_repo: Arc::new(MockSettingsRepo),
        inference_override: None,
        default_profile: None,
        cache_enabled: false,
        slot_dir: None,
        disk_budget: DiskBudget::Auto,
        agent_metrics: Arc::new(CacheMetricsStore::new()),
        observers: ProxyObservers::default(),
        access: ProxyAccessConfig::default(),
    }
}

/// Serve `config` on its own task.
pub(crate) async fn spawn(config: ServeConfig) -> Spawned {
    start(None, config).await
}

/// [`spawn`], with `serve` running under `bounds` in place of the stream
/// bounds it runs under otherwise.
pub(crate) async fn spawn_under(bounds: StreamBounds, config: ServeConfig) -> Spawned {
    start(Some(bounds), config).await
}

async fn start(bounds: Option<StreamBounds>, config: ServeConfig) -> Spawned {
    let addr = config.listener.local_addr().unwrap();
    let cancel = config.cancel.clone();
    let serving = async move {
        gglib_proxy::serve(config).await.ok();
    };
    let handle = match bounds {
        Some(bounds) => tokio::spawn(TEST_STREAM_BOUNDS.scope(bounds, serving)),
        None => tokio::spawn(serving),
    };

    // `serve` stamps its start time when its task first runs, and a test that
    // writes a slot file next needs that stamp to be the older of the two.
    tokio::time::sleep(Duration::from_millis(50)).await;
    Spawned {
        base: format!("http://{addr}"),
        port: addr.port(),
        cancel,
        handle,
    }
}
