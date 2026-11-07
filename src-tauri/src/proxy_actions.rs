//! Turning the endpoint (the proxy) on and off from outside a request.
//!
//! Both native menus do it here, not through the webview: the tray is
//! reachable when the window is hidden, and during autostart it runs before
//! any webview exists at all, so an event the frontend turns into an HTTP call
//! could go unheard. The macOS menu's Turn On / Off takes the same path so the
//! two cannot behave differently.
//!
//! So these call the daemon's `/api/proxy/*` routes directly — the same routes
//! the frontend uses — and then ask `daemon::watch` for an immediate poll.
//! They deliberately do **not** publish what they expect to be true: the
//! watcher is the only writer of the snapshot, so a poll already in flight
//! cannot overwrite an optimistic guess with a reading taken before the call.

use tauri::{AppHandle, Manager};
use tracing::{debug, error};

use crate::app::AppState;
use crate::app::events::{emit_or_log, names};

/// Start the proxy if it is not already running, returning the bound port.
///
/// Idempotent: the daemon's start route treats an already-running proxy as
/// success, and honours the saved `proxy_port` setting.
pub(crate) async fn start(app: &AppHandle) -> Result<u16, String> {
    let state = app.state::<AppState>();

    let status = state
        .daemon
        .post_json("/api/proxy/start", &serde_json::json!({}))
        .await?;
    let port = status
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .ok_or_else(|| "daemon reported no proxy port".to_string())?;

    state.refresh.now();

    Ok(port)
}

/// Stop the proxy, treating an already-stopped proxy as success (the
/// daemon's stop route is idempotent).
pub(crate) async fn stop(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    state
        .daemon
        .post_json("/api/proxy/stop", &serde_json::json!({}))
        .await?;

    state.refresh.now();

    Ok(())
}

/// Turn the endpoint off if the last poll saw it on, else on; off the event
/// thread.
///
/// The menus' one Turn On / Off item, labelled from the same snapshot this
/// reads. A failed turn-on is put on screen in the daemon's words ("Port 8080
/// is already in use …" names the fix); a failed turn-off is only logged,
/// since the stop route is idempotent and a failure there means the endpoint
/// is already gone.
pub(crate) fn spawn_switch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let on = app.state::<AppState>().snapshot.read().await.proxy_running;
        let result = if on {
            stop(&app).await
        } else {
            start(&app).await.map(|_| ())
        };
        let Err(e) = result else { return };
        error!(error = %e, on, "Turning the endpoint on or off failed");
        if !on {
            crate::tray::report_failure(&app, "Could not turn the endpoint on", &e);
        }
    });
}

/// Put the endpoint URL on the clipboard.
///
/// Shared by the tray menu and the macOS application menu. Both surfaces
/// disable the action while the proxy is stopped, so there is no default port
/// to fall back to and the snapshot is the only source of the port.
///
/// Goes through the frontend because clipboard access is a webview capability.
pub(crate) async fn copy_endpoint_url(app: &AppHandle) {
    let state = app.state::<AppState>();
    let url = state.snapshot.read().await.endpoint_url();

    if let Some(url) = url {
        emit_or_log(app, names::MENU_COPY_TO_CLIPBOARD, url);
    } else {
        debug!("Copy endpoint URL with no proxy listening - nothing to copy");
    }
}
