//! Starting and stopping the proxy from outside a request.
//!
//! The application menu drives the proxy by emitting an event the frontend
//! turns into an HTTP call. That works while a webview is loaded and
//! listening, which is exactly what the tray cannot assume: it is reachable
//! when the window is hidden, and during autostart it runs before any webview
//! exists at all.
//!
//! So these call the daemon's `/api/proxy/*` routes directly — the same routes
//! the frontend uses — and then ask `daemon::watch` for an immediate poll.
//! They deliberately do **not** publish what they expect to be true: the
//! watcher is the only writer of the snapshot, so a poll already in flight
//! cannot overwrite an optimistic guess with a reading taken before the call.

use tauri::{AppHandle, Manager};
use tracing::debug;

use crate::app::AppState;
use crate::app::events::{emit_or_log, names};

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

    state.refresh.now();

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

    state.refresh.now();

    Ok(())
}

/// Put the endpoint URL on the clipboard.
///
/// Shared by the tray menu and the macOS application menu, which had a copy of
/// this each — including a copy each of a default port to fall back on. Both
/// surfaces disable the action while the proxy is stopped, so there is nothing
/// to fall back to and the snapshot is the only source of the port.
///
/// Goes through the frontend because clipboard access is a webview capability.
pub async fn copy_endpoint_url(app: &AppHandle) {
    let state = app.state::<AppState>();
    let url = state.snapshot.read().await.endpoint_url();

    match url {
        Some(url) => emit_or_log(app, names::MENU_COPY_TO_CLIPBOARD, url),
        None => debug!("Copy endpoint URL with no proxy listening - nothing to copy"),
    }
}
