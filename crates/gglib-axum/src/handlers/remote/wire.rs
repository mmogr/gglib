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
}

impl RemoteEnableBody {
    pub(crate) fn into_request(self) -> EnableRequest {
        EnableRequest {
            allow_mcp: self.allow_mcp,
            relay: self.relay,
            discovery: self.discovery.unwrap_or(true),
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
    /// The six-digit pairing code.
    pub code: String,
    /// `<ticket>-<code>`, the one string a laptop pastes.
    pub pairing: String,
    /// Seconds the code lives unused.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub expires_in_s: u64,
}

impl From<Enabled> for RemoteEnableResponse {
    fn from(e: Enabled) -> Self {
        Self {
            ticket: e.ticket,
            code: e.code,
            pairing: e.pairing,
            expires_in_s: e.expires_in_s,
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
}

impl From<Connected> for RemoteConnectResponse {
    fn from(c: Connected) -> Self {
        Self {
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
}

impl From<ConnectSnapshot> for RemoteConnection {
    fn from(c: ConnectSnapshot) -> Self {
        Self {
            port: c.port,
            base_url: c.base_url,
            ticket_fingerprint: c.ticket_fingerprint,
            path: c.path,
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_body_is_the_safe_default() {
        let req = RemoteEnableBody::default().into_request();
        assert!(!req.allow_mcp);
        assert!(req.relay.is_none());
        assert!(req.discovery, "discovery is on unless switched off");
    }

    #[test]
    fn discovery_off_is_carried_through() {
        let body: RemoteEnableBody =
            serde_json::from_str(r#"{"allow_mcp":true,"discovery":false}"#).unwrap();
        let req = body.into_request();
        assert!(req.allow_mcp);
        assert!(!req.discovery);
    }

    #[test]
    fn an_empty_connect_body_reuses_the_last_ticket_on_a_free_port() {
        let req = RemoteConnectBody::default().into_request();
        assert!(req.pairing.is_none());
        assert!(req.port.is_none());
        assert!(req.discovery);
    }

    /// The status never carries the ticket, whatever the snapshot holds.
    #[test]
    fn the_status_has_no_field_that_could_hold_a_ticket() {
        let status = RemoteStatus::from(RemoteStatusSnapshot {
            enabled: true,
            ticket_fingerprint: Some("3ca82708b995".to_owned()),
            stored_ticket_fingerprint: Some("aabbccddeeff".to_owned()),
            ..RemoteStatusSnapshot::default()
        });
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"ticket_fingerprint\":\"3ca82708b995\""));
        assert!(!json.contains("\"ticket\":"), "{json}");
        assert!(!json.contains("pipe"), "{json}");
    }
}
