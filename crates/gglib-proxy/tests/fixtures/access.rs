//! The real proxy, served under an access policy the test chooses.

use std::sync::Arc;

use gglib_core::ProxyAccessConfig;
use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::common::{EmptyCatalog, MockSettingsRepo, NoopRuntime, make_mcp_service};

/// Spawn the real `gglib_proxy::serve` under a given access policy.
///
/// No upstream is configured, and the runtime launches nothing. `/v1/models`
/// is served from the (empty) catalog.
///
/// Returns `(proxy_base_url, port, cancel)`. The port is returned separately
/// for tests that construct authorities by hand.
pub(crate) async fn spawn_proxy(access: ProxyAccessConfig) -> (String, u16, CancellationToken) {
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
            // Device memory readable: no test here is about the fit.
            true,
            runtime,
            catalog,
            make_mcp_service(),
            cancel_clone,
            None, // daemon_cancel: no daemon in tests
            Arc::new(MockSettingsRepo),
            None,  // inference_override
            None,  // default_profile
            false, // cache_enabled
            None,  // slot_dir
            gglib_proxy::slot_eviction::DiskBudget::Auto,
            Arc::new(gglib_core::cache_metrics::CacheMetricsStore::new()),
            gglib_proxy::ProxyObservers::default(),
            &access,
        )
        .await
        .ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (format!("http://{addr}"), addr.port(), cancel)
}
