//! Bind-address resolution for `gglib web`.
//!
//! Pure decision logic: given the CLI flags and the stored settings, work out
//! which host to bind, whether LAN sharing is active, and which CORS policy
//! that implies. Kept free of I/O so the precedence rules are unit-testable.

use std::net::IpAddr;

use anyhow::{Result, bail};
use gglib_core::{CorsConfig, Settings};

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
/// Host precedence is CLI flag → stored `bind_host` → [`DEFAULT_BIND_HOST`].
/// Sharing is on when the flag is passed, or when the stored `share_lan` says
/// so; a bare `--share-lan` is the only thing clap can report, so the flag
/// alone cannot turn sharing *off* — use `gglib config settings set
/// --share-lan false` to clear a stored preference.
///
/// Neither flag is written back to settings; both are per-run overrides.
///
/// When sharing is on and no host was named anywhere, the bind widens to
/// `0.0.0.0`; an explicitly named non-loopback host wins over that widening, so
/// a multi-homed machine can share on a single interface.
///
/// # Errors
///
/// Returns an error when LAN sharing is requested against a loopback address —
/// nothing on the network could reach it, and mDNS would advertise a dead
/// address.
pub fn resolve_bind(
    cli_host: Option<String>,
    cli_share_lan: bool,
    settings: &Settings,
) -> Result<BindDecision> {
    let share_lan = cli_share_lan || settings.share_lan.unwrap_or(false);

    // Only a host nobody named gets widened to the wildcard address; a stored
    // `bind_host` counts as named, exactly like the flag.
    let named_host = cli_host.or_else(|| settings.bind_host.clone());

    let host = match (&named_host, share_lan) {
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

    /// Stored settings with only the two binding fields set.
    fn stored(bind_host: Option<&str>, share_lan: Option<bool>) -> Settings {
        Settings {
            bind_host: bind_host.map(ToOwned::to_owned),
            share_lan,
            ..Settings::default()
        }
    }

    /// Nothing set anywhere is the secure default: loopback bind,
    /// localhost-only CORS.
    #[test]
    fn test_defaults_to_localhost_only() {
        let decision = resolve_bind(None, false, &stored(None, None)).expect("resolves");
        assert_eq!(decision.host, "127.0.0.1");
        assert!(!decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::LocalOnly);
    }

    #[test]
    fn test_share_lan_flag_widens_to_all_interfaces() {
        let decision = resolve_bind(None, true, &stored(None, None)).expect("resolves");
        assert_eq!(decision.host, "0.0.0.0");
        assert!(decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::AllowAll);
    }

    #[test]
    fn test_cli_host_overrides_stored_setting() {
        let decision = resolve_bind(
            Some("192.168.1.100".to_owned()),
            false,
            &stored(Some("10.0.0.1"), None),
        )
        .expect("resolves");
        assert_eq!(decision.host, "192.168.1.100");
        assert_eq!(decision.cors, CorsConfig::LocalOnly);
    }

    /// An explicitly named LAN address is a narrower form of the same intent,
    /// so it wins over the wildcard widening while sharing stays on.
    #[test]
    fn test_explicit_host_wins_over_share_lan_widening() {
        let decision = resolve_bind(Some("192.168.1.50".to_owned()), true, &stored(None, None))
            .expect("a non-loopback host combined with --share-lan is a legitimate narrowing");
        assert_eq!(decision.host, "192.168.1.50");
        assert!(decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::AllowAll);
    }

    #[test]
    fn test_share_lan_with_loopback_is_rejected() {
        for host in ["127.0.0.1", "localhost", "::1", "127.0.0.2"] {
            let err = resolve_bind(Some(host.to_owned()), true, &stored(None, None))
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
        let decision =
            resolve_bind(Some("0.0.0.0".to_owned()), true, &stored(None, None)).expect("resolves");
        assert_eq!(decision.host, "0.0.0.0");
        assert!(decision.share_lan);
    }

    // ── Settings fallback ───────────────────────────────────────────────

    #[test]
    fn test_stored_bind_host_used_when_flag_absent() {
        let decision = resolve_bind(None, false, &stored(Some("10.0.0.5"), None)).expect("resolves");
        assert_eq!(decision.host, "10.0.0.5");
    }

    /// `gglib config settings set --share-lan true` must make a later bare
    /// `gglib web` bind the wildcard address — the persistence path #561 asks
    /// for, routed through settings rather than through the flag.
    #[test]
    fn test_stored_share_lan_used_when_flag_absent() {
        let decision = resolve_bind(None, false, &stored(None, Some(true))).expect("resolves");
        assert_eq!(decision.host, "0.0.0.0");
        assert!(decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::AllowAll);
    }

    /// A stored host is "named", so sharing does not widen it to the wildcard.
    #[test]
    fn test_stored_host_and_stored_share_lan_combine() {
        let decision =
            resolve_bind(None, false, &stored(Some("192.168.1.50"), Some(true))).expect("resolves");
        assert_eq!(decision.host, "192.168.1.50");
        assert!(decision.share_lan);
    }

    /// The loopback conflict applies to stored values too, so a bare
    /// `gglib web` cannot silently advertise an unreachable address.
    #[test]
    fn test_stored_loopback_conflicts_with_share_lan() {
        resolve_bind(None, false, &stored(Some("127.0.0.1"), Some(true)))
            .expect_err("stored loopback host must conflict with stored share_lan");
    }

    /// Setting `share-lan false` returns the server to localhost-only.
    #[test]
    fn test_stored_share_lan_false_restores_localhost_only() {
        let decision = resolve_bind(None, false, &stored(None, Some(false))).expect("resolves");
        assert!(!decision.share_lan);
        assert_eq!(decision.host, "127.0.0.1");
        assert_eq!(decision.cors, CorsConfig::LocalOnly);
    }

    /// A bare `--share-lan` cannot express "off", so the flag still wins over
    /// a stored `false`.
    #[test]
    fn test_flag_turns_sharing_on_over_stored_false() {
        let decision = resolve_bind(None, true, &stored(None, Some(false))).expect("resolves");
        assert!(decision.share_lan);
        assert_eq!(decision.host, "0.0.0.0");
    }
}
