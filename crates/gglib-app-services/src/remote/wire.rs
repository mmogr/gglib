//! The remote tunnel's daemon API as one set of shapes, which `ts-rs`
//! exports for the GUI. This file has what the tunnel reports, which the
//! daemon serialises and the CLI reads: the status, its device rows, and what
//! a `forget` did. [`wire_exchange`](super::wire_exchange) has the enable and
//! join exchanges.
//!
//! What is absent is the point. The ticket and the pairing code appear in one
//! response, the enable answer, which `invite` answers with too. The status is
//! a `GET`, the verb anything can call twice, so it carries fingerprints and
//! never a ticket, and its device rows have no field a key could live in.
//!
//! Every field but [`RemoteStatus::enabled`] and [`RemoteDevice::id`] is
//! `#[serde(default)]`: an answer that lacks one still reads, with that field
//! at its default.

use serde::{Deserialize, Serialize};

/// `GET /api/remote/status`, and the answer to `disable`, `disconnect` and
/// `kill`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[allow(
    clippy::struct_excessive_bools,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub struct RemoteStatus {
    /// Whether the serve side is up.
    pub enabled: bool,
    /// Fingerprint of the current ticket. Never the ticket.
    #[serde(default)]
    pub ticket_fingerprint: Option<String>,
    /// Whether a pairing code is still redeemable.
    #[serde(default)]
    pub pairing_active: bool,
    /// Whether a device redeemed the code now on offer. Per code, not per
    /// session: offering a new one clears it. Says nothing about the roster.
    #[serde(default)]
    pub paired: bool,
    /// Aggregate transport path: `idle`, `direct`, `relayed`.
    #[serde(default)]
    pub path: Option<String>,
    /// Every connected peer.
    #[serde(default)]
    pub peers: Vec<RemotePeer>,
    /// Whether tunnelled requests may reach `/mcp`.
    #[serde(default)]
    pub mcp_allowed: bool,
    /// Requests that arrived through the tunnel since the daemon started.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub tunnelled_requests: u64,
    /// Unix milliseconds of the last tunnelled request.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub last_tunnelled_ms: Option<i64>,
    /// The peer that sent it.
    #[serde(default)]
    pub last_peer: Option<String>,
    /// The connect side, when this machine is reaching another.
    #[serde(default)]
    pub connected: Option<RemoteConnection>,
    /// Fingerprint of the ticket a bare `join` would dial, from the stored
    /// pairing. Never the ticket.
    #[serde(default)]
    pub stored_ticket_fingerprint: Option<String>,
    /// Whether this machine holds a key from an earlier pairing.
    #[serde(default)]
    pub has_remote_key: bool,
    /// Whether this machine comes back reachable after a restart: the switch
    /// `enable` and `disable` set. It can disagree with `enabled` in either
    /// direction: on with nothing bound is a machine still arming, or one
    /// that failed to arm at boot; bound with the switch off cannot outlive
    /// the process.
    #[serde(default)]
    pub remote_enabled: bool,
    /// Where the endpoint key is kept. Deleting it retires this machine's
    /// address and revokes no device: the tunnel comes up at a new address
    /// that still admits every device key the roster lists. Retiring a device
    /// is a `DELETE` of `/api/remote/devices/{device}`.
    #[serde(default)]
    pub identity_path: Option<String>,
    /// Every device this machine has issued a key to. Carried on the status
    /// rather than fetched separately: the roster is a settings field this
    /// call has already read, so a client re-reading the status gets the
    /// list with it, and `admitted` cannot disagree with the `enabled`
    /// beside it.
    #[serde(default)]
    pub devices: Vec<RemoteDevice>,
}

/// One connected peer, by fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemotePeer {
    /// Twelve hex characters — the same name the daemon log uses.
    #[serde(default)]
    pub fingerprint: String,
    /// `direct` or `relayed`.
    #[serde(default)]
    pub path: String,
}

/// The connect side, while it is up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemoteConnection {
    /// The loopback port bound here.
    #[serde(default)]
    pub port: u16,
    /// `http://127.0.0.1:<port>/v1`.
    #[serde(default)]
    pub base_url: String,
    /// Fingerprint of the ticket dialled.
    #[serde(default)]
    pub ticket_fingerprint: String,
    /// How this side is reaching the peer: `idle`, `direct`, `relayed`.
    #[serde(default)]
    pub path: String,
    /// Seconds the far machine has been away, or `None` while it is here.
    /// The port stays bound either way.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub away_for_s: Option<u64>,
}

/// One device this machine has issued a key to: a row of the roster, or a
/// key the key file holds with no row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemoteDevice {
    /// The name the tunnel edge holds this device's key under, and the value
    /// it sends back on every request. Not a secret.
    pub id: String,
    /// What the device called itself when it joined, if it said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub label: Option<String>,
    /// Unix milliseconds at which this device's invite was minted; zero for a
    /// key with no row.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub joined_at: i64,
    /// Unix milliseconds at which a device redeemed the invite, or `null` if
    /// none ever has.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub redeemed_at: Option<i64>,
    /// Unix milliseconds of the last request that arrived under its key.
    /// Advisory, and written at most once a minute per device.
    #[serde(default)]
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub last_seen: Option<i64>,
    /// The fingerprint of the endpoint that redeemed this device's invite, or
    /// `null` if none was recorded. A record, not a check: a device that does
    /// not keep its endpoint key presents a new fingerprint every time it
    /// connects.
    #[serde(default)]
    pub peer: Option<String>,
    /// Whether the edge is admitting it right now, or `null` when the tunnel
    /// is down — nothing admits then, and `false` would read as "this one
    /// device was dropped".
    #[serde(default)]
    pub admitted: Option<bool>,
    /// Whether the roster lists this device. `false` is a key this machine
    /// holds with no row for it: `id` and `admitted` are all that is known of
    /// it, and a `DELETE` of it retires the key. A row that does not say is a
    /// roster row.
    #[serde(default = "a_roster_row")]
    pub recorded: bool,
    /// What is known of this row as a person reads it, written by the daemon
    /// on its own clock so every surface prints the same words. Empty in an
    /// answer that lacks it.
    #[serde(default)]
    pub description: String,
    /// Whether a device has arrived under this row: a redemption or a request
    /// is recorded against it. `false` for an invite nobody took and for a
    /// key with no row.
    #[serde(default)]
    pub joined: bool,
}

/// What [`RemoteDevice::recorded`] reads as when a row does not say.
const fn a_roster_row() -> bool {
    true
}

/// What `DELETE /api/remote/devices/{device}` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemoteForgotten {
    /// Whether this machine held anything under that name.
    ///
    /// `false` is a `200`, not a `404`: retiring a device that is already
    /// gone is the outcome asked for. A surface that wants to say "no such
    /// device" has this to say it with; one that just wants the device gone
    /// can ignore it.
    #[serde(default)]
    pub forgotten: bool,
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
