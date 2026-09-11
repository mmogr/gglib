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
    /// Offer a pairing code as well as bringing the tunnel up.
    ///
    /// `enable` is a switch and `invite` is what pairs a device, so the two
    /// are separate verbs — this is the flag that lets a first run be one
    /// command rather than two. A restart never sets it: the daemon putting
    /// the tunnel back has no audience, and a code nobody is watching for is
    /// a live grant nobody spends.
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
    /// The port this machine tried first and could not have — the remembered
    /// one, or the default — when it had to take another. `None` when the
    /// address stayed put.
    pub moved_from: Option<u16>,
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
    /// How long the far machine has been away — idle past the grace, still
    /// being dialled — or `None` while it is here. The port stays bound
    /// either way; this is what tells a person which.
    pub away_for_s: Option<u64>,
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
    /// Whether a device redeemed the code now on offer.
    ///
    /// Per code, not per session: offering a new one clears it, so a second
    /// `invite` against a live tunnel does not inherit the first device's
    /// answer. Says nothing about the roster — a machine with devices paired
    /// months ago reports `false` until it offers a code and one is taken.
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
    /// Fingerprint of the machine a bare `connect` would dial, from the
    /// stored pairing — and, because that pairing is one record, the machine
    /// the stored key belongs to. `None` when nothing is stored, or when the
    /// ticket is from a format this build cannot read.
    pub stored_ticket_fingerprint: Option<String>,
    /// Whether this machine holds a key from an earlier pairing. Never true
    /// on its own: a stored pairing is a ticket *and* a key.
    pub has_remote_key: bool,
    /// Whether this machine will put its proxy back on the tunnel after a
    /// restart — the switch `enable`/`disable` set, read from settings.
    ///
    /// Distinct from [`Self::enabled`], which is whether a tunnel is bound
    /// *right now*. They disagree in both directions and both are worth
    /// seeing: on with nothing bound is a machine still arming, or one that
    /// failed to arm at boot; bound with the switch off cannot outlive the
    /// process.
    pub remote_enabled: bool,
    /// Where this machine's endpoint key is kept.
    ///
    /// Always present now — the identity lasts (ADR 0012 decision 4,
    /// reversed) — and shown because revoking a ticket is deleting this
    /// file, which is a thing a person cannot do without being told where
    /// it is.
    pub identity_path: Option<String>,
}

/// One row of the device list: a device, and whether the tunnel in front
/// of this machine is currently holding its key.
#[derive(Debug, Clone)]
pub struct DeviceView {
    /// The name the edge holds its key under, and the value it sends back in
    /// `X-Modelpipe-Device`. Not secret: it travels on every request.
    pub id: String,
    /// What the device called itself when it joined, when it said.
    pub label: Option<String>,
    /// Unix milliseconds at which its invite was minted — not when the
    /// device redeemed it, and an invite nobody redeems keeps the row.
    pub joined_at: i64,
    /// Unix milliseconds at which a device redeemed this row's invite, or
    /// `None` if none ever has — an invite that was minted and never taken.
    pub redeemed_at: Option<i64>,
    /// Unix milliseconds of the last request that arrived under its key,
    /// written at most once a minute. Advisory: a local process can forge
    /// the marker headers, though not to name a device this machine never
    /// issued a key to.
    pub last_seen: Option<i64>,
    /// Whether the live listener holds this device's key — `None` when the
    /// tunnel is down, because then nothing admits and a `false` would read
    /// as this one device having been dropped.
    pub admitted: Option<bool>,
}
