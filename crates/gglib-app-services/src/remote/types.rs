//! What `RemoteOps` is asked for, and what `enable`, `invite` and `join` hand
//! back.
//!
//! Plain structs. The daemon API's shapes are in `wire.rs` and
//! `wire_exchange.rs`; the second converts to and from these.

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
    /// Offer a pairing code as well as bringing the tunnel up.
    ///
    /// `enable` is a switch and `invite` is what pairs a device, so the two
    /// are separate verbs — this is the flag that lets a first run be one
    /// command rather than two. A restart never sets it: the daemon putting
    /// the tunnel back has no audience, and a code nobody is watching for is
    /// a live code nobody spends.
    pub invite: bool,
}

/// What `enable` hands back, exactly once: the ticket, and the pairing code
/// when one was asked for. Shown to a person now and never re-read.
#[derive(Debug, Clone)]
pub struct Enabled {
    /// The ticket, canonical string form.
    pub ticket: String,
    /// The six-digit pairing code, when `enable` was asked to offer one.
    ///
    /// `None` is the ordinary case: the tunnel is up and no device is being
    /// paired right now: only an `enable` asked to invite offers one.
    pub pairing: Option<OfferedPairing>,
    /// Whether tunnelled requests may reach `/mcp` on the session this call
    /// ended up talking about.
    ///
    /// Reported rather than echoed, because the two can differ: an
    /// `enable --invite` against a tunnel that is already up offers a code
    /// and leaves the flags alone, so a caller that repeated its own
    /// `--allow-mcp` back to the operator would be describing a grant the
    /// daemon did not make.
    pub mcp_allowed: bool,
    /// Whether this call found the tunnel already up and answered from that
    /// session rather than arming one. That includes an `enable` that waited
    /// while the daemon's own resume brought the session back; without
    /// `invite` it carries no pairing.
    ///
    /// The flags of the request are ignored on that path — the session's
    /// grants belong to the `enable` that armed it — so a caller cannot say
    /// what it changed without knowing which happened. Inferring it from
    /// [`Self::mcp_allowed`] only works when the caller asked for `/mcp` and
    /// was told no; every other combination is indistinguishable, which is
    /// why this is a field and not a guess.
    pub already_up: bool,
}

/// A pairing code on offer, and what is worth knowing about it.
///
/// Named for what it is rather than `Pairing`, which in this module tree
/// already means the session's live redemption state.
#[derive(Debug, Clone)]
pub struct OfferedPairing {
    /// The six-digit code.
    pub code: String,
    /// The string a laptop pastes: `<ticket>-<code>`.
    pub pairing: String,
    /// Seconds the code lives unused.
    pub expires_in_s: u64,
    /// The device this code will issue a key to, once it is redeemed.
    pub device: String,
}

/// What `join` is asked for.
#[derive(Debug, Clone, Default)]
pub struct JoinRequest {
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

/// What `join` hands back.
#[derive(Debug, Clone)]
pub struct Joined {
    /// The loopback port that is now the far machine.
    pub port: u16,
    /// `http://127.0.0.1:<port>/v1`, ready to paste into a client.
    pub base_url: String,
    /// Fingerprint of the ticket dialled.
    pub ticket_fingerprint: String,
    /// Whether this call redeemed a pairing code and stored the key, as
    /// opposed to reusing a key from an earlier pairing.
    pub paired: bool,
    /// The port this machine tried first and could not have — the remembered
    /// one, or the default — when it had to take another. `None` when the
    /// address stayed put.
    pub moved_from: Option<u16>,
}
