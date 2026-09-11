//! Following a key rotation into the running listener.

use std::sync::Arc;

use gglib_core::services::{AppCore, SETTINGS_CACHE_TTL};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Follow `proxy_api_key` so a rotation reaches the running listener.
///
/// There is no settings-changed event in gglib and there cannot be a useful
/// one — the CLI writes the same database from another process — so this
/// polls on the settings cache's own cadence, which bounds the staleness to
/// the same window the proxy already accepts. A cleared setting is ignored,
/// because clearing is not a rotation and there is no new token to follow.
/// Do not read that as the proxy holding the line: a listener that bound on
/// loopback has no floor, so a clear reopens it and the tunnel edge is then
/// the only door still asking (ADR 0012, decision 2).
pub(super) async fn rotation_poll(
    core: Arc<AppCore>,
    handle: Arc<modelpipe::ServeHandle>,
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
        // `set_backend_auth`, never `set_token`. The listener runs
        // `TokenPolicy::Named`, and `set_token` would give it a primary it
        // does not have — silently turning the per-device listener back into
        // a shared-key one on the first rotation, with no error and no log
        // line, and making `forget` incomplete from then on.
        //
        // Devices are untouched by this: they hold their own keys, and what
        // rotates is only what the edge presents to the backend in their
        // place. Rotating the proxy's key no longer un-pairs anybody.
        match handle.set_backend_auth(Some(next.clone())) {
            Ok(()) => {
                info!("remote tunnel now presents the rotated API key to the proxy");
                current = next;
            }
            Err(e) => warn!("remote tunnel refused the rotated API key: {e}"),
        }
    }
}
