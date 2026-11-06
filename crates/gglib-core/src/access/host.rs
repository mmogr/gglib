use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Whether `host` refers to the loopback interface.
///
/// Covers the literal `localhost` alongside any address that parses as an IP
/// and is loopback, so `::1` and `127.0.0.2` are caught as well as `127.0.0.1`.
///
/// Accepts a bare host, with or without a port and with or without IPv6
/// brackets — callers reach this both with a configured bind host and with a
/// raw `Host` header, and the two are spelled differently.
#[must_use]
pub fn is_loopback_host(host: &str) -> bool {
    let Some(host) = normalize_host(host) else {
        return false;
    };
    host == "localhost" || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Whether `host` is a wildcard ("all interfaces") address.
///
/// True for both `0.0.0.0` and its IPv6 equivalent `::`, so the two are treated
/// alike when deciding how to describe the bind and how to advertise it.
#[must_use]
pub fn is_wildcard_host(host: &str) -> bool {
    let Some(host) = normalize_host(host) else {
        return false;
    };
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_unspecified())
}

/// Reduce a host or authority to a bare, comparable host name.
///
/// Lowercases, strips a trailing `:port`, and unwraps IPv6 brackets, so the
/// three spellings a single address arrives in — `[::1]:8080` from a browser,
/// `::1` from a config file, `[::1]` from a URL — compare equal.
///
/// Returns `None` for anything that cannot be a host: an empty string, or a
/// value carrying userinfo or a path. Those are rejected rather than
/// sanitized, because a `Host` header containing them is malformed and the
/// only safe reading of a malformed claim is no claim at all.
#[must_use]
pub fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim();
    if host.is_empty() || host.contains('@') || host.contains('/') {
        return None;
    }

    // Bracketed IPv6, with or without a port: `[::1]` / `[::1]:8080`.
    let bare = if let Some(rest) = host.strip_prefix('[') {
        let (inner, tail) = rest.split_once(']')?;
        if !tail.is_empty() && !tail.starts_with(':') {
            return None;
        }
        inner
    } else {
        // A single colon is a port separator. Several mean an unbracketed IPv6
        // literal, which is malformed in a `Host` header but unambiguous as a
        // configured value, so it is taken whole rather than truncated at the
        // first colon.
        match host.split_once(':') {
            Some((h, port)) if !port.contains(':') => h,
            _ => host,
        }
    };

    if bare.is_empty() {
        return None;
    }
    Some(bare.to_ascii_lowercase())
}

/// Where a client on this machine reaches an endpoint bound to `addr`.
///
/// A wildcard bind (`0.0.0.0` or `::`) names every interface and no reachable
/// address, so it is reached on loopback; any other bind is reached where it
/// is. The run door dials this and the proxy's status reports it, so the
/// address gglib prints is the one it uses.
#[must_use]
pub fn reachable_at(addr: SocketAddr) -> SocketAddr {
    if addr.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
    } else {
        addr
    }
}

/// The base URL a client is given for an endpoint bound to `addr`.
///
/// `http://<reachable address>/v1`: the daemon's proxy status carries it as
/// `base_url`, `gglib up` prints it, and the GUI shows it as it is.
#[must_use]
pub fn endpoint_base_url(addr: SocketAddr) -> String {
    format!("http://{}/v1", reachable_at(addr))
}
