//! The enable and join exchanges of the daemon API: what `POST
//! /api/remote/enable` and `POST /api/remote/join` are asked, and what they
//! answer. `invite` answers with the enable response.
//!
//! Beside [`wire`](super::wire), which has what the tunnel reports. The CLI
//! sends the two bodies and the daemon reads them, the daemon sends the two
//! answers and the CLI reads them, `ts-rs` exports all four, and every field
//! is `#[serde(default)]`.

use serde::{Deserialize, Serialize};

use super::types::{EnableRequest, Enabled, JoinRequest, Joined};

/// Body for `POST /api/remote/enable`. Every field optional; an empty body
/// is the default: no `/mcp`, public relays, discovery on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[allow(
    clippy::pub_underscore_fields,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub struct RemoteEnableBody {
    /// Let tunnelled requests reach `/mcp`. Off unless asked for.
    #[serde(default)]
    pub allow_mcp: bool,
    /// A self-hosted relay URL; omitted uses the public relays.
    #[serde(default)]
    pub relay: Option<String>,
    /// Publish to and resolve through n0's discovery service. Omitted is on.
    #[serde(default)]
    pub discovery: Option<bool>,
    /// Accepted and ignored.
    ///
    /// The identity is always kept, so this asks for what it already gets.
    /// Its removal is #1043.
    #[serde(default, rename = "keep_identity")]
    pub _keep_identity: bool,
    /// Offer a pairing code as well as bringing the tunnel up.
    ///
    /// `enable` is a switch and `invite` is what pairs a device, so a first
    /// run is two commands unless this is set. Omitted is off, which is what
    /// a restart wants: a code nobody is watching for is a live code nobody
    /// spends.
    #[serde(default)]
    pub invite: bool,
}

impl RemoteEnableBody {
    /// What [`RemoteOps::enable`](super::RemoteOps::enable) is asked for.
    #[must_use]
    pub fn into_request(self) -> EnableRequest {
        EnableRequest {
            allow_mcp: self.allow_mcp,
            relay: self.relay,
            discovery: self.discovery.unwrap_or(true),
            invite: self.invite,
        }
    }
}

/// What `POST /api/remote/enable` and `POST /api/remote/invite` answer,
/// once. The ticket and the code are shown to a person now and are not
/// retrievable afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemoteEnableResponse {
    /// The ticket, canonical lowercase form.
    #[serde(default)]
    pub ticket: String,
    /// The six-digit pairing code, when one was asked for.
    ///
    /// Absent is the ordinary case — the tunnel is up and nothing is being
    /// paired. A surface must render the absence as "no code", not as an
    /// expired one: an empty string here would read as a pairing that had
    /// already run out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub code: Option<String>,
    /// `<ticket>-<code>`, the one string a laptop pastes, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub pairing: Option<String>,
    /// Seconds the code lives unused, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional, type = "number"))]
    pub expires_in_s: Option<u64>,
    /// The device the code will issue a key to, when there is one: the id
    /// `gglib remote list` shows and `forget` takes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub device: Option<String>,
    /// Whether tunnelled requests may reach `/mcp` on the session this call
    /// ended up talking about — which is not always the one the caller asked
    /// for. An `--invite` against a tunnel that is already up leaves the
    /// flags alone, so a surface that echoed the request back would state a
    /// grant the daemon did not make.
    #[serde(default)]
    pub mcp_allowed: bool,
    /// Whether the tunnel was already up and this answered from that session
    /// rather than arming one — true for every `invite`, for an `enable` with
    /// `invite` set that found one running, and for an `enable` that waited
    /// while the daemon's own resume brought one back, which carries no code
    /// unless `invite` was set. The request's other flags were ignored on
    /// that path, and a surface cannot infer it: only "asked for `/mcp`, told
    /// no" shows in the rest of the answer.
    #[serde(default)]
    pub already_up: bool,
}

impl From<Enabled> for RemoteEnableResponse {
    fn from(e: Enabled) -> Self {
        let pairing = e.pairing;
        Self {
            ticket: e.ticket,
            code: pairing.as_ref().map(|p| p.code.clone()),
            pairing: pairing.as_ref().map(|p| p.pairing.clone()),
            expires_in_s: pairing.as_ref().map(|p| p.expires_in_s),
            device: pairing.map(|p| p.device),
            mcp_allowed: e.mcp_allowed,
            already_up: e.already_up,
        }
    }
}

/// Body for `POST /api/remote/join`. An empty body dials the ticket this
/// machine last connected to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemoteJoinBody {
    /// `<ticket>-<code>` for a first pairing, a bare ticket afterwards,
    /// omitted to reuse the last one.
    #[serde(default)]
    pub pairing: Option<String>,
    /// The loopback port to bind; omitted picks a free one.
    #[serde(default)]
    pub port: Option<u16>,
    /// A self-hosted relay URL for this side; omitted uses the public relays.
    #[serde(default)]
    pub relay: Option<String>,
    /// Resolve through n0's discovery service. Omitted is on.
    #[serde(default)]
    pub discovery: Option<bool>,
}

impl RemoteJoinBody {
    /// What [`RemoteOps::join`](super::RemoteOps::join) is asked for.
    #[must_use]
    pub fn into_request(self) -> JoinRequest {
        JoinRequest {
            pairing: self.pairing,
            port: self.port,
            relay: self.relay,
            discovery: self.discovery.unwrap_or(true),
        }
    }
}

/// What `POST /api/remote/join` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct RemoteJoinResponse {
    /// The loopback port that is now the far machine.
    #[serde(default)]
    pub port: u16,
    /// `http://127.0.0.1:<port>/v1`, ready for a client.
    #[serde(default)]
    pub base_url: String,
    /// Fingerprint of the ticket dialled.
    #[serde(default)]
    pub ticket_fingerprint: String,
    /// Whether this call redeemed a pairing code and stored the key.
    #[serde(default)]
    pub paired: bool,
    /// The port this machine wanted and could not have, when it had to take
    /// another; `None` when the address stayed put.
    #[serde(default)]
    pub moved_from: Option<u16>,
}

impl From<Joined> for RemoteJoinResponse {
    fn from(j: Joined) -> Self {
        Self {
            port: j.port,
            base_url: j.base_url,
            ticket_fingerprint: j.ticket_fingerprint,
            paired: j.paired,
            moved_from: j.moved_from,
        }
    }
}

#[cfg(test)]
#[path = "wire_exchange_tests.rs"]
mod wire_exchange_tests;
