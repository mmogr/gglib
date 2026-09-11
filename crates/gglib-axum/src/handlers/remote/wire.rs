//! The remote tunnel's request and response shapes.
//!
//! Split from the handlers the way `proxy/wire.rs` is, and for one more
//! reason: what is *not* in these shapes is the point. The ticket appears in
//! exactly one response — `enable`'s — and the pairing code likewise; the
//! status carries a fingerprint and never the ticket, because `GET` is the
//! verb anything can call twice.

use gglib_app_services::{
    ConnectRequest, ConnectSnapshot, Connected, EnableRequest, Enabled, RemoteStatusSnapshot,
};

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
    /// Whether a device redeemed the code this session.
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
    /// Where the endpoint key is kept; deleting it revokes the ticket.
    pub identity_path: Option<String>,
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
        }
    }
}

#[cfg(test)]
#[path = "wire_tests.rs"]
mod wire_tests;
