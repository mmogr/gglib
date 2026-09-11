//! What `GET /api/remote/status` reports.
//!
//! Split from `wire.rs` on the seam its own module doc names: that file is
//! what the tunnel is *asked* — enable, connect, kill — and this is what it
//! *says*. The split is also what keeps either under the 300-line budget
//! `scripts/check_rust_complexity.sh` allows a file not in its baseline.
//!
//! What is absent is the point. `GET` is the verb anything can call twice,
//! so the status carries fingerprints and never a ticket, and device rows
//! that have no field a key could live in.

use gglib_app_services::{ConnectSnapshot, RemoteStatusSnapshot};

use super::devices::RemoteDevice;

/// The connect side, while it is up.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteConnection {
    /// The loopback port bound here.
    pub port: u16,
    /// `http://127.0.0.1:<port>/v1`.
    pub base_url: String,
    /// Fingerprint of the ticket dialled.
    pub ticket_fingerprint: String,
    /// How this side is reaching the peer: `idle`, `direct`, `relayed`.
    pub path: String,
    /// Seconds the far machine has been away, or `None` while it is here.
    /// The port stays bound either way.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub away_for_s: Option<u64>,
}

impl From<ConnectSnapshot> for RemoteConnection {
    fn from(c: ConnectSnapshot) -> Self {
        Self {
            port: c.port,
            base_url: c.base_url,
            ticket_fingerprint: c.ticket_fingerprint,
            path: c.path,
            away_for_s: c.away_for_s,
        }
    }
}

/// One connected peer, by fingerprint.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemotePeer {
    /// Twelve hex characters — the same name the daemon log uses.
    pub fingerprint: String,
    /// `direct` or `relayed`.
    pub path: String,
}

/// `GET /api/remote/status` and the `disable` response.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteStatus {
    /// Whether the serve side is up.
    pub enabled: bool,
    /// Fingerprint of the current ticket. Never the ticket.
    pub ticket_fingerprint: Option<String>,
    /// Whether a pairing code is still redeemable.
    pub pairing_active: bool,
    /// Whether a device redeemed the code now on offer. Per code, not per
    /// session, and says nothing about the roster.
    pub paired: bool,
    /// Aggregate transport path: `idle`, `direct`, `relayed`.
    pub path: Option<String>,
    /// Every connected peer.
    pub peers: Vec<RemotePeer>,
    /// Whether tunnelled requests may reach `/mcp`.
    pub mcp_allowed: bool,
    /// Requests that arrived through the tunnel since the daemon started.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub tunnelled_requests: u64,
    /// Unix milliseconds of the last tunnelled request.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub last_tunnelled_ms: Option<i64>,
    /// The peer that sent it.
    pub last_peer: Option<String>,
    /// The connect side, when this machine is reaching another.
    pub connected: Option<RemoteConnection>,
    /// Fingerprint of the ticket a bare `connect` would dial. Never the
    /// ticket.
    pub stored_ticket_fingerprint: Option<String>,
    /// Whether this machine holds a key from an earlier pairing.
    pub has_remote_key: bool,
    /// Whether this machine comes back reachable after a restart.
    pub remote_enabled: bool,
    /// Where the endpoint key is kept; deleting it revokes the ticket for
    /// *every* device at once. Retiring one device is a `DELETE` of
    /// `/api/remote/devices/{device}`.
    pub identity_path: Option<String>,
    /// Every device this machine has issued a key to. Carried on the status
    /// rather than fetched separately: the roster is a settings field this
    /// call has already read, so a client re-reading the status gets the
    /// list with it, and `admitted` cannot disagree with the `enabled`
    /// beside it.
    pub devices: Vec<RemoteDevice>,
}

impl From<RemoteStatusSnapshot> for RemoteStatus {
    fn from(s: RemoteStatusSnapshot) -> Self {
        Self {
            enabled: s.enabled,
            ticket_fingerprint: s.ticket_fingerprint,
            pairing_active: s.pairing_active,
            paired: s.paired,
            path: s.path,
            peers: s
                .peers
                .into_iter()
                .map(|(fingerprint, path)| RemotePeer { fingerprint, path })
                .collect(),
            mcp_allowed: s.mcp_allowed,
            tunnelled_requests: s.tunnelled_requests,
            last_tunnelled_ms: s.last_tunnelled_ms,
            last_peer: s.last_peer,
            connected: s.connected.map(RemoteConnection::from),
            stored_ticket_fingerprint: s.stored_ticket_fingerprint,
            has_remote_key: s.has_remote_key,
            remote_enabled: s.remote_enabled,
            identity_path: s.identity_path,
            devices: s.devices.into_iter().map(RemoteDevice::from).collect(),
        }
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod status_tests;
