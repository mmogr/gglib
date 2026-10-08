//! The real proxy, served under an access policy the test chooses.

use gglib_core::ProxyAccessConfig;
use gglib_proxy::ServeConfig;
use tokio_util::sync::CancellationToken;

use super::spawn::{defaults, spawn};

/// Spawn the real `gglib_proxy::serve` under a given access policy.
///
/// No upstream is configured, and the runtime launches nothing. `/v1/models`
/// is served from the (empty) catalog.
///
/// Returns `(proxy_base_url, port, cancel)`. The port is returned separately
/// for tests that construct authorities by hand.
pub(crate) async fn spawn_proxy(access: ProxyAccessConfig) -> (String, u16, CancellationToken) {
    let proxy = spawn(ServeConfig {
        access,
        ..defaults().await
    })
    .await;
    (proxy.base, proxy.port, proxy.cancel)
}
