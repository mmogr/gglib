//! Step 5 of [`super::run_daemon`]: the endpoint comes on with the service.
//!
//! The proxy belongs to the daemon, so the switch is honoured here and not
//! in the desktop app: whoever starts the service, the app, `gglib up` or any
//! other command, the endpoint comes on the same way. The start is the one
//! every caller that wants the endpoint up makes,
//! [`ProxyOps::ensure_running`](gglib_app_services::ProxyOps::ensure_running),
//! with the stored address, port and cache.
//!
//! It runs after the management API is bound and does not hold it up: a
//! port that is taken leaves the service running with the endpoint off, and
//! the reason, naming the port, goes to the daemon's log.

use std::net::SocketAddr;

use tracing::{info, warn};

use crate::state::AppState;

/// What the service's start did about the endpoint.
#[derive(Debug)]
pub(super) enum Autostart {
    /// The switch is off, so the endpoint waits to be turned on.
    Off,
    /// The endpoint is on, at this address.
    On(SocketAddr),
    /// The endpoint should have come on and did not; why.
    Failed(String),
}

/// Bring the endpoint up if the switch says so, log what happened, and say
/// it.
pub(super) async fn autostart_proxy(state: &AppState) -> Autostart {
    let outcome = start_if_on(state).await;
    match &outcome {
        Autostart::Off => {}
        Autostart::On(addr) => info!("proxy autostarted on {addr}"),
        Autostart::Failed(reason) => warn!("proxy autostart failed: {reason}"),
    }
    outcome
}

async fn start_if_on(state: &AppState) -> Autostart {
    let settings = match state.core.settings().get().await {
        Ok(settings) => settings,
        Err(e) => return Autostart::Failed(format!("could not read settings: {e}")),
    };
    if !settings.effective_proxy_autostart() {
        return Autostart::Off;
    }
    match state.proxy.ensure_running().await {
        Ok(addr) => Autostart::On(addr),
        Err(e) => Autostart::Failed(e.to_string()),
    }
}

#[cfg(test)]
#[path = "autostart_tests.rs"]
mod autostart_tests;
