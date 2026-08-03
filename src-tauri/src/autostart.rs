//! Always-on proxy startup and OS login-item registration.
//!
//! The proxy has always been a feature you switch on from inside the app,
//! which makes it awkward as an endpoint for other clients: something has to
//! be open and something has to have been clicked. The two settings handled
//! here — `proxy_autostart` and `start_at_login` — turn it into a background
//! service instead, and together with `close_to_tray` mean the endpoint is
//! simply there.
//!
//! Both operations are best-effort. Neither is worth failing a launch over:
//! an app that refuses to open because a login item could not be registered
//! is strictly worse than one that opens and says so in the log.

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use tracing::{error, info};

use crate::app::AppState;
use crate::menu::state_sync::sync_menu_state_internal;

/// Bring the proxy up when `proxy_autostart` is set, and register or
/// unregister the login item to match `start_at_login`.
///
/// Runs after the window exists so a slow or failing start delays nothing the
/// user is waiting on.
pub async fn apply(app: &AppHandle) {
    let state = app.state::<AppState>();

    let settings = match state.core.settings().get().await {
        Ok(settings) => settings,
        Err(e) => {
            error!(error = %e, "Could not read settings; skipping autostart");
            return;
        }
    };

    apply_login_item(app, settings.start_at_login == Some(true));

    if settings.proxy_autostart == Some(true) {
        start_proxy(app, &state).await;
    }
}

/// Start the proxy and publish the result to the rest of the app.
///
/// Uses `ensure_running`, which is idempotent and reads the saved
/// `proxy_port`: a user with a standing `gglib proxy` on that port gets their
/// own process left alone rather than a bind conflict at every launch.
async fn start_proxy(app: &AppHandle, state: &tauri::State<'_, AppState>) {
    match state.proxy.ensure_running().await {
        Ok(address) => {
            info!(port = address.port(), "Proxy started automatically");
            *state.proxy_enabled.write().await = true;
            *state.proxy_port.write().await = Some(address.port());

            // The webview has not loaded yet, so the frontend's usual
            // `set_proxy_state` round-trip has not run. Publish directly or
            // the tray would show the proxy as stopped while it is serving.
            if let Err(e) = sync_menu_state_internal(app, state).await {
                error!(error = %e, "Failed to sync menu state after proxy autostart");
            }
        }
        Err(e) => {
            // A failure here is recoverable: the user can still start the
            // proxy by hand, and the most likely cause is a port already
            // taken by something they started themselves.
            error!(error = %e, "Proxy autostart failed; continuing without it");
        }
    }
}

/// Register or unregister the OS login item, logging rather than failing.
///
/// Called on every launch rather than only when the setting changes, so a
/// login item removed outside gglib is restored instead of leaving the stored
/// value describing something that is no longer true.
fn apply_login_item(app: &AppHandle, enabled: bool) {
    let manager = app.autolaunch();

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    match result {
        Ok(()) => info!(enabled, "Login item synchronised"),
        Err(e) => error!(error = %e, enabled, "Could not update login item"),
    }
}
