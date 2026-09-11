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
    /// The code was live and unspent; here is the key it stood for and the
    /// name the tunnel edge holds that key under. The code is dead from
    /// this moment.
    ///
    /// `device` is not a secret and `key` is: the proxy hands the key
    /// straight back to the device that redeemed and keeps neither.
    Granted {
        /// The device's own bearer, issued by `invite`.
        key: String,
        /// The name the edge admits that bearer under, which arrives on
        /// every later request as `X-Modelpipe-Device`.
        device: String,
    },
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
    /// the code, when the request carried one — a per-process value on the
    /// connecting side, so it names a run rather than a device. `name` is
    /// what that device calls itself, when it said; it is stored for a
    /// person to read and sent nowhere.
    fn redeem_pairing_code(
        &self,
        code: &str,
        peer: Option<&str>,
        name: Option<&str>,
    ) -> PairingOutcome;

    /// Whether requests arriving through the tunnel may reach `/mcp`.
    fn mcp_allowed(&self) -> bool;

    /// A request marked as tunnelled reached the proxy. Counted, and the
    /// peer remembered, for the status surface; never the request itself.
    ///
    /// `device` is the named token the edge says admitted it, when it named
    /// one. Absent means a grant admitted the request — or that a local
    /// process forged the markers, which is why nothing is ever granted on
    /// the strength of it.
    fn note_tunnelled_request(&self, peer: Option<&str>, device: Option<&str>);
}
