use std::net::IpAddr;

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
