//! The proxy's view of the remote tunnel.
//!
//! `gglib-app-services` owns the tunnel (ADR 0012) and `gglib-proxy` cannot
//! depend on it — the dependency runs the other way. What the proxy needs
//! from the tunnel is small and fits a port: be told that a request arrived
//! through the tunnel at all, and say which MCP servers the live session
//! opens to paired devices. `/mcp` asks it nothing, because no tunnelled
//! request reaches `/mcp`. Pairing is not part of it either: the tunnel edge
//! answers a pairing request itself and never forwards it. Everything else
//! about the tunnel stays where it lives.
//!
//! # Design Rules
//!
//! - No iroh or modelpipe types: the proxy learns what it is told and never
//!   what the transport is.
//! - Synchronous. Every implementation is a lock and a counter, and a port
//!   that forces an `await` on the request path for that would be paying
//!   for nothing.
//! - `Debug` is a supertrait because the config that carries this derives
//!   it.

use std::sync::Arc;

/// What the proxy may ask the tunnel's owner.
pub trait RemoteGatewayPort: Send + Sync + std::fmt::Debug {
    /// The MCP servers, by the names `gglib remote enable --allow-mcp`
    /// gave, that the live session opens to paired devices; empty with the
    /// tunnel down. A paired device's runs are offered their tools.
    fn mcp_servers(&self) -> Arc<[String]>;

    /// A request marked as tunnelled reached the proxy. Counted, and the
    /// peer remembered, for the status surface; never the request itself.
    ///
    /// `device` is the named token the edge says admitted it, when it named
    /// one. Absent means a client that reached the proxy directly forged the
    /// markers, since the edge names a device on everything it forwards,
    /// which is why nothing is ever granted on the strength of it.
    fn note_tunnelled_request(&self, peer: Option<&str>, device: Option<&str>);
}
