//! What the remote tunnel is asked, and what those calls answer.
//!
//! Split from the handlers the way `proxy/wire.rs` is, and for one more
//! reason: what is *not* in these shapes is the point. The ticket appears in
//! exactly one response shape — `enable`'s, which `invite` reuses — and the
//! pairing code likewise; the status carries a fingerprint and never the
//! ticket, because `GET` is the verb anything can call twice.

use gglib_app_services::{ConnectRequest, Connected, EnableRequest, Enabled};

/// Body for `POST /api/remote/enable`. Every field optional; an empty body
/// is the default: no `/mcp`, public relays, discovery on.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteEnableBody {
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
    /// The identity is always kept now, so this asks for what it already
    /// gets. Kept on the wire for one release so a desktop app or script
    /// built against the old shape still posts a body this deserialises
    /// rather than getting a 422 for a field that no longer means anything;
    /// removing it is a follow-up, not a surprise.
    #[serde(default, rename = "keep_identity")]
    pub _keep_identity: bool,
    /// Offer a pairing code as well as bringing the tunnel up.
    ///
    /// `enable` is a switch and `invite` is what pairs a device, so a first
    /// run is two commands unless this is set. Omitted is off, which is what
    /// a restart wants: a code nobody is watching for is a live grant nobody
    /// spends.
    #[serde(default)]
    pub invite: bool,
}

impl RemoteEnableBody {
    pub(crate) fn into_request(self) -> EnableRequest {
        EnableRequest {
            allow_mcp: self.allow_mcp,
            relay: self.relay,
            discovery: self.discovery.unwrap_or(true),
            invite: self.invite,
        }
    }
}

/// What `POST /api/remote/enable` answers, once. The ticket and the code are
/// shown to a person now and are not retrievable afterwards.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteEnableResponse {
    /// The ticket, canonical lowercase form.
    pub ticket: String,
    /// The six-digit pairing code, when one was asked for.
    ///
    /// Absent is the ordinary case — the tunnel is up and nothing is being
    /// paired. A surface must render the absence as "no code", not as an
    /// expired one: an empty string here would read as a pairing that had
    /// already run out.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// `<ticket>-<code>`, the one string a laptop pastes, when there is one.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pairing: Option<String>,
    /// Seconds the code lives unused, when there is one.
    #[cfg_attr(feature = "ts-bindings", ts(optional, type = "number"))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_in_s: Option<u64>,
    /// The device the code will issue a key to, when there is one.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    /// Whether tunnelled requests may reach `/mcp` on the session this call
    /// ended up talking about — which is not always the one the caller asked
    /// for. An `--invite` against a tunnel that is already up leaves the
    /// flags alone, so a surface that echoed the request back would state a
    /// grant the daemon did not make.
    pub mcp_allowed: bool,
    /// Whether the tunnel was already up and this answered from that session
    /// rather than arming one — true for every `invite`, for an `enable` with
    /// `invite` set that found one running, and for an `enable` that waited
    /// while the daemon's own resume brought one back, which carries no code
    /// unless `invite` was set. The request's other flags were ignored on
    /// that path, and a surface cannot infer it: only "asked for `/mcp`, told
    /// no" shows in the rest of the answer.
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

/// Body for `POST /api/remote/connect`. An empty body dials the ticket this
/// machine last connected to.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteConnectBody {
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

impl RemoteConnectBody {
    pub(crate) fn into_request(self) -> ConnectRequest {
        ConnectRequest {
            pairing: self.pairing,
            port: self.port,
            relay: self.relay,
            discovery: self.discovery.unwrap_or(true),
        }
    }
}

/// What `POST /api/remote/connect` answers.
#[derive(Debug, Clone, serde::Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteConnectResponse {
    /// The loopback port that is now the far machine.
    pub port: u16,
    /// `http://127.0.0.1:<port>/v1`, ready for a client.
    pub base_url: String,
    /// Fingerprint of the ticket dialled.
    pub ticket_fingerprint: String,
    /// Whether this call redeemed a pairing code and stored the key.
    pub paired: bool,
    /// The port this machine wanted and could not have, when it had to take
    /// another; `None` when the address stayed put.
    pub moved_from: Option<u16>,
}

impl From<Connected> for RemoteConnectResponse {
    fn from(c: Connected) -> Self {
        Self {
            moved_from: c.moved_from,
            port: c.port,
            base_url: c.base_url,
            ticket_fingerprint: c.ticket_fingerprint,
            paired: c.paired,
        }
    }
}

/// Body for `POST /api/remote/kill`. The word is required, as it is on the
/// proxy route this forwards to: a one-way door is not opened by an empty
/// `POST`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub(crate) struct RemoteKillBody {
    /// Must be the literal `"shutdown"`.
    #[serde(default)]
    pub confirm: Option<String>,
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
