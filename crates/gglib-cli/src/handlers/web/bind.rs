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

/// Whether `host` is a wildcard ("all interfaces") address.
///
/// True for both `0.0.0.0` and its IPv6 equivalent `::`, so the two are treated
/// alike when deciding how to describe the bind and how to advertise it.
pub fn is_wildcard(host: &str) -> bool {
    host.parse::<IpAddr>().is_ok_and(|ip| ip.is_unspecified())
}

/// Format `host:port` as an HTTP authority, bracketing IPv6 literals.
///
/// A bare IPv6 address is ambiguous in a URL (`http://::1:9887`), so anything
/// that parses as IPv6 is wrapped: `http://[::1]:9887`.
pub fn http_authority(host: &str, port: u16) -> String {
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V6(ip)) => format!("[{ip}]:{port}"),
        _ => format!("{host}:{port}"),
    }
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
/// `0.0.0.0`; a named non-loopback host wins over that widening, so a
/// multi-homed machine can share on a single interface.
///
/// A loopback host cannot be combined with sharing — nothing on the network
/// could reach it, and mDNS would advertise a dead address. When the loopback
/// host came from *stored settings* but sharing was asked for on the command
/// line, the command line is the fresher intent: the stored host is dropped and
/// the bind widens. A stale saved preference must not veto an explicit flag.
///
/// # Errors
///
/// Returns an error when sharing is combined with a loopback host that the user
/// named on the command line, or when both values come from stored settings and
/// contradict each other.
pub fn resolve_bind(
    cli_host: Option<String>,
    cli_share_lan: bool,
    settings: &Settings,
) -> Result<BindDecision> {
    let share_lan = cli_share_lan || settings.share_lan.unwrap_or(false);

    // A host nobody named gets widened to the wildcard when sharing.
    let host = match (cli_host, settings.bind_host.clone()) {
        // Named on the command line: authoritative, so a conflict is an error.
        (Some(host), _) if share_lan && is_loopback(&host) => {
            bail!(
                "--share-lan cannot be combined with the loopback address '{host}': \
                 no other device on the network could reach it. \
                 Drop --share-lan for localhost-only access, or bind a LAN address \
                 (e.g. --host 0.0.0.0)."
            );
        }
        (Some(host), _) => host,

        // Stored loopback host, sharing asked for on the command line: the flag
        // wins over the saved fallback, exactly as --host would.
        (None, Some(host)) if cli_share_lan && is_loopback(&host) => {
            tracing::info!(
                "ignoring stored bind-host '{host}' because --share-lan was passed; \
                 binding {ALL_INTERFACES} instead"
            );
            ALL_INTERFACES.to_owned()
        }

        // Both stored and contradictory — the saved configuration cannot be
        // satisfied, and no command-line input says which side to prefer.
        (None, Some(host)) if share_lan && is_loopback(&host) => {
            bail!(
                "stored settings are contradictory: share-lan is enabled but \
                 bind-host is the loopback address '{host}', which no other device \
                 on the network can reach. \
                 Fix with `gglib config settings set --bind-host 0.0.0.0`, or \
                 `--share-lan false` for localhost-only access."
            );
        }
        (None, Some(host)) => host,

        (None, None) if share_lan => ALL_INTERFACES.to_owned(),
        (None, None) => DEFAULT_BIND_HOST.to_owned(),
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

    /// An explicit wildcard is not loopback, so it passes the conflict check —
    /// for both address families.
    #[test]
    fn test_explicit_wildcard_is_allowed_with_share_lan() {
        for host in ["0.0.0.0", "::"] {
            let decision =
                resolve_bind(Some(host.to_owned()), true, &stored(None, None)).expect("resolves");
            assert_eq!(decision.host, host);
            assert!(decision.share_lan);
        }
    }

    /// `::` is the IPv6 "all interfaces" address and must be classified the
    /// same as `0.0.0.0` — it drives both the startup banner and whether mDNS
    /// auto-detects addresses.
    #[test]
    fn test_is_wildcard_covers_both_families() {
        assert!(is_wildcard("0.0.0.0"));
        assert!(is_wildcard("::"));
        assert!(!is_wildcard("127.0.0.1"));
        assert!(!is_wildcard("::1"));
        assert!(!is_wildcard("192.168.1.50"));
        assert!(!is_wildcard("localhost"));
    }

    /// A bare IPv6 authority is ambiguous in a URL, so it gets bracketed.
    #[test]
    fn test_http_authority_brackets_ipv6() {
        assert_eq!(http_authority("127.0.0.1", 9887), "127.0.0.1:9887");
        assert_eq!(http_authority("localhost", 9887), "localhost:9887");
        assert_eq!(http_authority("::1", 9887), "[::1]:9887");
        assert_eq!(http_authority("::", 9887), "[::]:9887");
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

    /// Two stored values that contradict each other cannot be satisfied, and
    /// nothing on the command line says which to prefer — so a bare `gglib web`
    /// errors rather than silently advertising an unreachable address.
    #[test]
    fn test_contradictory_stored_settings_are_rejected() {
        let err = resolve_bind(None, false, &stored(Some("127.0.0.1"), Some(true)))
            .expect_err("stored loopback host must conflict with stored share_lan");
        assert!(
            err.to_string().contains("contradictory"),
            "error should point at the saved configuration: {err}"
        );
    }

    /// A stored loopback host is only a fallback for "no --host given", so it
    /// must not veto an explicit `--share-lan`; the flag is the fresher intent
    /// and the bind widens instead of erroring.
    #[test]
    fn test_cli_share_lan_overrides_stored_loopback_host() {
        let decision = resolve_bind(None, true, &stored(Some("127.0.0.1"), None))
            .expect("an explicit flag outranks a stale stored fallback");
        assert_eq!(decision.host, "0.0.0.0");
        assert!(decision.share_lan);
        assert_eq!(decision.cors, CorsConfig::AllowAll);

        // Also when the stored preference already had sharing on.
        let decision = resolve_bind(None, true, &stored(Some("localhost"), Some(true)))
            .expect("the flag still wins");
        assert_eq!(decision.host, "0.0.0.0");
    }

    /// The override is narrow: a stored *non*-loopback host is still honoured
    /// when --share-lan is passed, so single-interface sharing keeps working.
    #[test]
    fn test_cli_share_lan_keeps_stored_non_loopback_host() {
        let decision =
            resolve_bind(None, true, &stored(Some("192.168.1.50"), None)).expect("resolves");
        assert_eq!(decision.host, "192.168.1.50");
        assert!(decision.share_lan);
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
