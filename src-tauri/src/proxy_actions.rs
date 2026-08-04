//! Starting and stopping the proxy from outside a request.
//!
//! The application menu drives the proxy by emitting an event the frontend
//! turns into an HTTP call. That works while a webview is loaded and
//! listening, which is exactly what the tray cannot assume: it is reachable
//! when the window is hidden, and during autostart it runs before any webview
//! exists at all.
//!
//! So these go straight to the same [`ProxyOps`] the embedded Axum server
//! uses, and then do by hand what the HTTP handler would have done for them —
//! publish the new state to `AppState` and broadcast the lifecycle event.
//! Skipping that broadcast is what leaves the UI showing a stopped proxy while
//! it is serving.
//!
//! [`ProxyOps`]: gglib_app_services::ProxyOps

use gglib_core::events::AppEvent;
use gglib_core::ports::AppEventEmitter;
use tauri::{AppHandle, Manager};
use tracing::error;

use crate::app::AppState;
use crate::menu::state_sync::sync_all_state;

/// Start the proxy if it is not already running, returning the bound port.
///
/// Idempotent, via `ProxyOps::ensure_running`: a proxy this app already
/// started is reported as success rather than as a conflict, and the saved
/// `proxy_port` is honoured so a standing `gglib proxy` on that port is left
/// alone instead of collected as a bind failure.
pub async fn start(app: &AppHandle) -> Result<u16, String> {
    let state = app.state::<AppState>();

    let address = state
        .proxy
        .ensure_running()
        .await
        .map_err(|e| e.to_string())?;

    publish(app, true, Some(address.port())).await;
    state.sse.emit(AppEvent::proxy_started(address.port()));

    Ok(address.port())
}

/// Stop the proxy, treating an already-stopped proxy as success.
pub async fn stop(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();

    // A stopped proxy is the state the caller asked for, so a NotRunning
    // conflict is not worth surfacing as a failure.
    if let Err(e) = state.proxy.stop().await
        && !matches!(e, gglib_app_services::GuiError::Conflict(_))
    {
        return Err(e.to_string());
    }

    publish(app, false, None).await;
    state.sse.emit(AppEvent::proxy_stopped());

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
