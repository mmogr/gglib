//! Bind-address resolution for `gglib web`.
//!
//! Pure decision logic: given the CLI flags, work out which host to bind,
//! whether LAN sharing is active, and which CORS policy that implies. Kept free
//! of I/O so the precedence rules are unit-testable.

use std::net::IpAddr;

use anyhow::{Result, bail};
use gglib_core::CorsConfig;

/// Compiled-in fallback used when the flag says nothing.
/// Matches `ServerConfig::with_defaults`.
pub const DEFAULT_BIND_HOST: &str = "127.0.0.1";

/// Wildcard address used when LAN sharing is on and no host was named.
const ALL_INTERFACES: &str = "0.0.0.0";

/// The resolved binding decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindDecision {
    /// Address to bind the HTTP listener to.
    pub host: String,
    /// Whether LAN sharing is active (drives the warning banner and mDNS).
    pub share_lan: bool,
    /// CORS policy implied by `share_lan`.
    pub cors: CorsConfig,
}

/// Whether `host` refers to the loopback interface.
///
/// Covers the literal `localhost` alongside any address that parses as an IP
/// and is loopback, so `::1` and `127.0.0.2` are caught as well as `127.0.0.1`.
fn is_loopback(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// Resolve the effective bind address and CORS policy.
///
/// The host comes from the flag, falling back to [`DEFAULT_BIND_HOST`]. When
/// sharing is on and no host was named, the bind widens to `0.0.0.0`; an
/// explicitly named non-loopback host wins over that widening, so a multi-homed
/// machine can share on a single interface.
///
/// # Errors
///
/// Returns an error when LAN sharing is requested against a loopback address —
/// nothing on the network could reach it, and mDNS would advertise a dead
/// address.
pub fn resolve_bind(cli_host: Option<String>, share_lan: bool) -> Result<BindDecision> {
    let host = match (&cli_host, share_lan) {
        (Some(host), true) if is_loopback(host) => {
            bail!(
                "--share-lan cannot be combined with the loopback address '{host}': \
                 no other device on the network could reach it. \
                 Drop --share-lan for localhost-only access, or bind a LAN address \
                 (e.g. --host 0.0.0.0)."
            );
        }
        (Some(host), _) => host.clone(),
        (None, true) => ALL_INTERFACES.to_owned(),
        (None, false) => DEFAULT_BIND_HOST.to_owned(),
    };

    Ok(BindDecision {
        host,
        share_lan,
        cors: if share_lan {
            CorsConfig::AllowAll
        } else {
            CorsConfig::LocalOnly
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No flags is the secure default: loopback bind, localhost-only CORS.
    #[test]
    fn test_defaults_to_localhost_only() {
        let decision = resolve_bind(None, false).expect("resolves");
        assert_eq!(decision.host, "127.0.0.1");
        assert!(!decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::LocalOnly);
    }

    #[test]
    fn test_share_lan_flag_widens_to_all_interfaces() {
        let decision = resolve_bind(None, true).expect("resolves");
        assert_eq!(decision.host, "0.0.0.0");
        assert!(decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::AllowAll);
    }

    #[test]
    fn test_cli_host_is_used_verbatim() {
        let decision = resolve_bind(Some("192.168.1.100".to_owned()), false).expect("resolves");
        assert_eq!(decision.host, "192.168.1.100");
        assert_eq!(decision.cors, CorsConfig::LocalOnly);
    }

    /// An explicitly named LAN address is a narrower form of the same intent,
    /// so it wins over the wildcard widening while sharing stays on.
    #[test]
    fn test_explicit_host_wins_over_share_lan_widening() {
        let decision = resolve_bind(Some("192.168.1.50".to_owned()), true)
            .expect("a non-loopback host combined with --share-lan is a legitimate narrowing");
        assert_eq!(decision.host, "192.168.1.50");
        assert!(decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::AllowAll);
    }

    #[test]
    fn test_share_lan_with_loopback_is_rejected() {
        for host in ["127.0.0.1", "localhost", "::1", "127.0.0.2"] {
            let err = resolve_bind(Some(host.to_owned()), true)
                .expect_err("loopback + --share-lan must be rejected");
            assert!(
                err.to_string().contains("--share-lan"),
                "error should name the offending flag: {err}"
            );
        }
    }

    /// An explicit `0.0.0.0` is not loopback, so it passes the conflict check.
    #[test]
    fn test_explicit_wildcard_is_allowed_with_share_lan() {
        let decision = resolve_bind(Some("0.0.0.0".to_owned()), true).expect("resolves");
        assert_eq!(decision.host, "0.0.0.0");
        assert!(decision.share_lan);
    }
}
