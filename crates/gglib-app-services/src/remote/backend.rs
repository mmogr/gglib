//! Which local address the tunnel fronts.
//!
//! `enable` reads the proxy's bound address exactly once, and this turns
//! that read — a *bind* address — into something `modelpipe::serve` will
//! dial.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Where the tunnel dials this machine's proxy, and on what terms.
pub(super) struct Backend {
    /// The backend URL handed to `modelpipe::serve`.
    pub(super) url: String,
    /// Whether that URL names an address on the operator's own network,
    /// which modelpipe will not dial unless it is told to expect one.
    pub(super) allow_private: bool,
}

impl Backend {
    /// The backend a proxy bound to `addr` is reached at.
    ///
    /// A bind address and a dial address are not the same thing, and
    /// modelpipe screens the address it dials (`locality::admits`) against a
    /// rule the proxy's bind never had to satisfy. Two cases differ:
    ///
    /// * A **wildcard** bind (`0.0.0.0`, `::`) names no host at all, so
    ///   modelpipe refuses it whatever it is told — on Linux, dialling it
    ///   reaches loopback, so admitting it would be an accident rather than
    ///   a decision. A proxy on the wildcard is listening on loopback as
    ///   well, so the loopback literal of the same family is what it meant.
    ///   The port is kept, which is the whole point of rewriting rather than
    ///   guessing at 8080.
    /// * A **LAN** bind (`192.168.…`, `10.…`, `fd00::…`) names a real
    ///   interface that the loopback literal would not reach, so it is
    ///   dialled as written — with the one flag modelpipe requires before it
    ///   will dial the operator's own network. That widens nothing else: the
    ///   URL carries a literal address, so the flag can only ever readmit
    ///   the address the proxy is already on.
    ///
    /// Everything else is passed through untouched and modelpipe decides.
    /// Link-local and public addresses stay refused however the flag is set,
    /// and this must not try to talk its way around that: a proxy bound to a
    /// routable address would be re-exporting a server this machine does not
    /// own, which is the case the rule exists for.
    pub(super) fn at(addr: SocketAddr) -> Self {
        let ip = match addr.ip() {
            IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            other => other,
        };
        Self {
            // `SocketAddr`'s `Display` brackets an IPv6 literal, which is
            // what a URL authority needs — and what modelpipe strips back
            // off the host before it resolves, so `http://[::1]:8080` is the
            // spelling that works rather than the one that looks safe.
            url: format!("http://{}", SocketAddr::new(ip, addr.port())),
            allow_private: is_private(ip),
        }
    }
}

/// Whether an address is one modelpipe classifies as private — RFC 1918 or
/// `fc00::/7`.
///
/// A mirror of `locality::classify`, whose verdict is the one that actually
/// decides; this side only has to predict it well enough to set the flag.
/// IPv4-mapped IPv6 is unwrapped first for the reason it is unwrapped there:
/// `::ffff:192.168.1.5` is a LAN address wearing an IPv6 hat, and every
/// per-family check answers the wrong question about it.
fn is_private(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => v4.is_private(),
        // fc00::/7, unique local. Matched by prefix because
        // `Ipv6Addr::is_unique_local` is still unstable.
        IpAddr::V6(v6) => v6.segments()[0] & 0xFE00 == 0xFC00,
    }
}

#[cfg(test)]
#[path = "backend_tests.rs"]
mod backend_tests;
