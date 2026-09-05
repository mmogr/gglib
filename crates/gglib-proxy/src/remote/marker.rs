//! Did this request come through the tunnel?
//!
//! The serve side sets two headers on every request it forwards, after
//! removing any copy the client sent: `Via: 1.1 modelpipe` and
//! `X-Modelpipe-Peer: <fingerprint>`. This middleware reads them into a
//! request extension so the `/mcp` gate and the pairing route can act on
//! them, and tells the tunnel's owner a request arrived.
//!
//! **Restrictive only.** A local client can write these headers too. What it
//! gains is a refusal on `/mcp` and a tick on a counter — nothing is granted
//! on the marker's say-so, and nothing ever should be. The direction that
//! matters holds: a tunnelled peer cannot make its request look local,
//! because the edge always overwrites.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};

use crate::server::AppState;

/// The pseudonym the tunnel edge writes into `Via`.
const VIA_PSEUDONYM: &str = "modelpipe";

/// The header carrying the connecting peer's fingerprint.
const PEER_HEADER: &str = "x-modelpipe-peer";

/// This request arrived through the tunnel.
#[derive(Debug, Clone)]
pub(crate) struct Tunnelled {
    /// The peer's fingerprint, when the edge sent a well-formed one: twelve
    /// hex characters, the same rule the tunnel's own log uses.
    pub(crate) peer: Option<Arc<str>>,
}

impl Tunnelled {
    /// Read the markers, if the `Via` names the tunnel.
    ///
    /// `Via` may carry several hops (`1.1 a, 1.1 modelpipe`); any entry whose
    /// received-by token is the pseudonym counts. The comparison is on the
    /// token alone, so a client cannot disguise one marker as another by
    /// padding it.
    pub(crate) fn from_headers(headers: &HeaderMap) -> Option<Self> {
        let via_names_tunnel = headers
            .get_all("via")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|value| value.split(','))
            .any(|hop| {
                // `received-protocol received-by [comment]`; the second
                // whitespace-separated token is the name.
                hop.split_whitespace()
                    .nth(1)
                    .is_some_and(|by| by.eq_ignore_ascii_case(VIA_PSEUDONYM))
            });
        if !via_names_tunnel {
            return None;
        }
        let peer = headers
            .get(PEER_HEADER)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|p| p.len() == 12 && p.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(|p| Arc::from(p.to_ascii_lowercase()));
        Some(Self { peer })
    }
}

/// Mark tunnelled requests and count them.
///
/// Installed with [`axum::middleware::from_fn_with_state`] as a `layer` on the
/// whole router, inside the Host guard, so a request the guard refuses is
/// never counted as one that arrived.
pub(crate) async fn remote_marker(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    if let Some(tunnelled) = Tunnelled::from_headers(req.headers()) {
        if let Some(gateway) = state.remote_gateway() {
            gateway.note_tunnelled_request(tunnelled.peer.as_deref());
        }
        req.extensions_mut().insert(tunnelled);
    }
    next.run(req).await
}

#[cfg(test)]
#[path = "marker_tests.rs"]
mod marker_tests;
