//! What the proxy's drawing answer tells a paired device beside whether it
//! can draw: the MCP servers whose tools a run from it is offered.
//!
//! A phone asks `GET /v1/images/drawing` on open, and it is the one answer
//! a device already reads before it sends a run, so the servers ride on it
//! (ADR 0024, decision 4). It is read on each ask, so a server opened or
//! closed since shows on the phone's next open (ADR 0025). The daemon's own
//! `GET /api/images/drawing` is not this: the page's runs are not a
//! device's.

use gglib_core::contracts::http::images::DrawingAvailability;
use serde::Serialize;

use crate::server::AppState;

/// [`DrawingAvailability`]'s fields, flattened, and `mcp_servers`: the
/// names of the servers open to paired devices (`gglib mcp allow-remote
/// <server>`) while a tunnel session is live; `[]` with none open, with no
/// session, or on a proxy with no tunnel owner. There whether or not it can
/// draw.
#[derive(Debug, Serialize)]
pub(crate) struct DeviceDrawing {
    #[serde(flatten)]
    answer: DrawingAvailability,
    mcp_servers: Vec<String>,
}

impl DeviceDrawing {
    /// `answer`, with the servers open to devices when `state`'s tunnel
    /// owner says a session is live.
    pub(crate) async fn of(state: &AppState, answer: DrawingAvailability) -> Self {
        let serving = state
            .remote_gateway()
            .is_some_and(|gateway| gateway.serving());
        let mcp_servers = if serving {
            state.mcp.device_server_names().await
        } else {
            Vec::new()
        };
        Self {
            answer,
            mcp_servers,
        }
    }
}
