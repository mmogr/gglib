//! Starting and stopping the proxy from outside a request.
//!
//! The application menu drives the proxy by emitting an event the frontend
//! turns into an HTTP call. That works while a webview is loaded and
//! listening, which is exactly what the tray cannot assume: it is reachable
//! when the window is hidden, and during autostart it runs before any webview
//! exists at all.
//!
//! So these call the daemon's `/api/proxy/*` routes directly — the same
//! routes the frontend uses — and then publish the new state to `AppState`
//! so the menu and tray repaint. The lifecycle SSE event is the daemon's to
//! broadcast; it does so from the handler these calls hit.

use tauri::{AppHandle, Manager};
use tracing::error;

use crate::app::AppState;
use crate::menu::state_sync::sync_all_state;

/// Start the proxy if it is not already running, returning the bound port.
///
/// Idempotent: the daemon's start route treats an already-running proxy as
/// success, and honours the saved `proxy_port` setting.
pub async fn start(app: &AppHandle) -> Result<u16, String> {
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

    publish(app, true, Some(port)).await;

    Ok(port)
}

/// Stop the proxy, treating an already-stopped proxy as success (the
/// daemon's stop route is idempotent).
pub async fn stop(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    state
        .daemon
        .post_json("/api/proxy/stop", &serde_json::json!({}))
        .await?;

    publish(app, false, None).await;

    Ok(())
}

/// Record proxy state on `AppState` and refresh anything that displays it.
///
/// Public because proxy state also arrives from the frontend, through
/// `commands::util::set_proxy_state`.
pub async fn publish(app: &AppHandle, running: bool, port: Option<u16>) {
    let state = app.state::<AppState>();

    *state.proxy_enabled.write().await = running;
    *state.proxy_port.write().await = port;

    if let Err(e) = sync_all_state(app, &state).await {
        error!(error = %e, "Failed to sync menu and tray state");
    }
}
