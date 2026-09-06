//! The proxy crash watcher.
//!
//! Split from `bootstrap.rs`, which is at its file-size budget, when the
//! remote tunnel joined the context it assembles. One job: turn the
//! supervisor's exit channel into a `ProxyCrashed` event on the SSE stream,
//! with no polling.

use std::sync::Arc;

use gglib_app_services::ProxyOps;
use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use gglib_runtime::proxy::ProxyStatus;

use crate::sse::SseBroadcaster;

/// Emit `ProxyCrashed` whenever the proxy task exits without being asked to.
pub(crate) fn spawn(proxy: &Arc<ProxyOps>, sse: &Arc<SseBroadcaster>) {
    let mut rx = proxy.exit_receiver();
    let sse = Arc::clone(sse);
    tokio::spawn(async move {
        // Skip the initial value; only react to actual changes.
        while rx.changed().await.is_ok() {
            let status = rx.borrow().clone();
            if status == ProxyStatus::Crashed {
                tracing::warn!("Proxy crash detected by watcher — emitting ProxyCrashed event");
                sse.emit(AppEvent::proxy_crashed());
            }
        }
    });
}
