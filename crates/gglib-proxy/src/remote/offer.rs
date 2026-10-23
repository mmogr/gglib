//! What the proxy's drawing answer tells a paired device beside whether it
//! can draw: the MCP servers whose tools a run from it is offered.
//!
//! A phone asks `GET /v1/images/drawing` on open, and it is the one answer
//! a device already reads before it sends a run, so the servers ride on it
//! (ADR 0024, decision 4). The daemon's own `GET /api/images/drawing` is
//! not this: the page's runs are not a device's.

use std::sync::Arc;

use gglib_core::contracts::http::images::DrawingAvailability;
use serde::Serialize;

use crate::server::AppState;

/// [`DrawingAvailability`]'s fields, flattened, and `mcp_servers`: the
/// names the live session opens to paired devices (`gglib remote enable
/// --allow-mcp <server>`), `[]` with none, with the tunnel down, or on a
/// proxy with no tunnel owner. There whether or not it can draw.
#[derive(Debug, Serialize)]
pub(crate) struct DeviceDrawing {
    #[serde(flatten)]
    answer: DrawingAvailability,
    mcp_servers: Vec<String>,
}

impl DeviceDrawing {
    /// `answer`, with the servers `state`'s tunnel owner says it opens.
    pub(crate) fn of(state: &AppState, answer: DrawingAvailability) -> Self {
        let opened = state
            .remote_gateway()
            .map_or_else(|| Arc::from([]), |gateway| gateway.mcp_servers());
        Self {
            answer,
            mcp_servers: opened.to_vec(),
        }
    }
}
