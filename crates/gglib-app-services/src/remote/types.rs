//! What `RemoteOps` is asked for and what it reports.
//!
//! Plain structs, not wire DTOs: `gglib-axum` maps these onto its own
//! `ts-rs`-exported types, so the daemon API can change shape without this
//! crate knowing.

/// What `enable` is asked for.
#[derive(Debug, Clone, Default)]
pub struct EnableRequest {
    /// Let requests arriving through the tunnel reach `/mcp`. Off by default
    /// (ADR 0012): a leaked token with a shell MCP server configured is
    /// remote code execution.
    pub allow_mcp: bool,
    /// A self-hosted relay URL for this endpoint; `None` uses the public
    /// relays.
    pub relay: Option<String>,
    /// Publish to and resolve through n0's discovery service. On by default;
    /// off removes that contact and the property that a ticket keeps working
    /// after this machine changes network.
    pub discovery: bool,
}

/// What `enable` hands back, exactly once: the ticket and the pairing code
/// are shown to a person now and never re-read.
#[derive(Debug, Clone)]
pub struct Enabled {
    /// The ticket, canonical string form.
    pub ticket: String,
    /// The six-digit pairing code.
    pub code: String,
    /// The pairing string a laptop pastes: `<ticket>-<code>`.
    pub pairing: String,
    /// Seconds the code lives unused.
    pub expires_in_s: u64,
}

/// What `connect` is asked for.
#[derive(Debug, Clone, Default)]
pub struct ConnectRequest {
    /// `<ticket>-<code>` for a first pairing, a bare ticket afterwards, or
    /// `None` to dial the ticket this machine last connected to.
    pub pairing: Option<String>,
    /// The loopback port to bind here; `None` picks a free one.
    pub port: Option<u16>,
    /// A self-hosted relay URL for this endpoint; `None` uses the public
    /// relays.
    pub relay: Option<String>,
    /// Resolve through n0's discovery service. Off means this side dials
    /// only the paths the ticket carries.
    pub discovery: bool,
}

/// What `connect` hands back.
#[derive(Debug, Clone)]
pub struct Connected {
    /// The loopback port that is now the far machine.
    pub port: u16,
    /// `http://127.0.0.1:<port>/v1`, ready to paste into a client.
    pub base_url: String,
    /// Fingerprint of the ticket dialled.
    pub ticket_fingerprint: String,
    /// Whether this call redeemed a pairing code and stored the key, as
    /// opposed to reusing a key from an earlier pairing.
    pub paired: bool,
}

/// The connect side, while it is up.
#[derive(Debug, Clone)]
pub struct ConnectSnapshot {
    /// The loopback port bound here.
    pub port: u16,
    /// `http://127.0.0.1:<port>/v1`.
    pub base_url: String,
    /// Fingerprint of the ticket dialled.
    pub ticket_fingerprint: String,
    /// How this side is reaching the peer: `idle`, `direct`, `relayed`.
    pub path: String,
}

/// The tunnel as the status surface sees it.
#[derive(Debug, Clone, Default)]
pub struct RemoteStatusSnapshot {
    /// Whether the serve side is up.
    pub enabled: bool,
    /// Fingerprint of the current ticket. Never the ticket.
    pub ticket_fingerprint: Option<String>,
    /// Whether a pairing code is still redeemable.
    pub pairing_active: bool,
    /// Whether a device redeemed the code this session.
    pub paired: bool,
    /// The aggregate transport path: `idle`, `direct`, `relayed`.
    pub path: Option<String>,
    /// Every connected peer, by fingerprint, with its own path.
    pub peers: Vec<(String, String)>,
    /// Whether tunnelled requests may reach `/mcp`.
    pub mcp_allowed: bool,
    /// Requests that arrived through the tunnel since the daemon started.
    pub tunnelled_requests: u64,
    /// Unix milliseconds of the last tunnelled request.
    pub last_tunnelled_ms: Option<i64>,
    /// The peer that sent it.
    pub last_peer: Option<String>,
    /// The connect side, when this machine is reaching another.
    pub connected: Option<ConnectSnapshot>,
    /// Fingerprint of the ticket a bare `connect` would dial, from settings.
    pub stored_ticket_fingerprint: Option<String>,
    /// Whether this machine holds a key from an earlier pairing.
    pub has_remote_key: bool,
}
