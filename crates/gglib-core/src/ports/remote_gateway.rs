//! The proxy's view of the remote tunnel.
//!
//! `gglib-app-services` owns the tunnel (ADR 0012) and `gglib-proxy` cannot
//! depend on it — the dependency runs the other way. What the proxy needs
//! from the tunnel is small and fits a port: redeem a pairing code, say
//! whether `/mcp` may be reached from outside, and be told that a request
//! arrived through the tunnel at all. Everything else about the tunnel
//! stays where it lives.
//!
//! # Design Rules
//!
//! - No iroh or modelpipe types: the proxy learns what it is told and never
//!   what the transport is.
//! - Synchronous. Every implementation is a lock and a counter, and a port
//!   that forces an `await` on the request path for that would be paying
//!   for nothing.
//! - `Debug` is a supertrait because the config that carries this derives
//!   it; an implementation must redact — a pending pairing code is a
//!   credential.

/// What redeeming a pairing code produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingOutcome {
    /// The code was live and unspent; here is the key it stood for. The
    /// code is dead from this moment.
    Granted(String),
    /// Wrong, expired, spent, exhausted, or nothing pending at all. One
    /// variant on purpose: telling them apart tells an attacker which of
    /// their guesses was close, and the route answers every one of them
    /// with the same flat refusal.
    Rejected,
}

/// What the proxy may ask the tunnel's owner.
pub trait RemoteGatewayPort: Send + Sync + std::fmt::Debug {
    /// Exchange a pairing code for the proxy's bearer token, once.
    ///
    /// `peer` is the tunnel edge's fingerprint for the device presenting
    /// the code, when the request carried one, so the owner can say which
    /// device paired.
    fn redeem_pairing_code(&self, code: &str, peer: Option<&str>) -> PairingOutcome;

    /// Whether requests arriving through the tunnel may reach `/mcp`.
    fn mcp_allowed(&self) -> bool;

    /// A request marked as tunnelled reached the proxy. Counted, and the
    /// peer remembered, for the status surface; never the request itself.
    fn note_tunnelled_request(&self, peer: Option<&str>);
}
