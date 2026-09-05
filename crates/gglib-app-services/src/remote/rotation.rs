//! Following a key rotation into the running listener.

use std::sync::Arc;

use gglib_core::services::{AppCore, SETTINGS_CACHE_TTL};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use super::gateway::RemoteGateway;

/// Follow `proxy_api_key` so a rotation reaches the running listener.
///
/// There is no settings-changed event in gglib and there cannot be a useful
/// one — the CLI writes the same database from another process — so this
/// polls on the settings cache's own cadence, which bounds the staleness to
/// the same window the proxy already accepts. A cleared setting is ignored:
/// the proxy keeps its floor, so the tunnel keeps its token.
pub(super) async fn rotation_poll(
    core: Arc<AppCore>,
    handle: Arc<modelpipe::ServeHandle>,
    gateway: Arc<RemoteGateway>,
    mut current: String,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            () = tokio::time::sleep(SETTINGS_CACHE_TTL) => {}
        }
        let stored = match core.settings().get().await {
            Ok(settings) => settings.proxy_api_key,
            Err(e) => {
                warn!("remote tunnel could not re-read settings: {e}");
                continue;
            }
        };
        let Some(next) = stored.filter(|k| !k.trim().is_empty()) else {
            continue;
        };
        if next == current {
            continue;
        }
        match handle.set_token(next.clone()) {
            Ok(()) => {
                info!("remote tunnel now enforces the rotated API key");
                gateway.pairing.update_key(next.clone());
                current = next;
            }
            Err(e) => warn!("remote tunnel refused the rotated API key: {e}"),
        }
    }
}
