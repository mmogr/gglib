#![doc = include_str!("README.md")]
mod bearer;
mod host;

#[cfg(test)]
mod host_tests;

pub use bearer::bearer_matches;
pub use host::{is_loopback_host, is_wildcard_host, normalize_host};

use crate::cors::CorsConfig;

/// Where the proxy's bearer token came from.
///
/// Carried alongside the resolved value so the startup banner can explain the
/// decision rather than merely stating it — the same `(value, source)` shape
/// [`resolve_context_size_with_source`](crate::server_config::resolve_context_size_with_source)
/// uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeySource {
    /// Supplied on the command line or through `GGLIB_API_KEY`.
    Flag,
    /// Read from the stored `proxy_api_key` setting.
    Settings,
    /// Minted by this run because the bind was not loopback and nothing else
    /// supplied one. Printed once, then persisted to settings.
    Generated,
    /// No token configured; the endpoint is unauthenticated.
    None,
}

/// Compare two byte strings without an early exit on the first difference, so
/// response timing does not leak how many leading bytes of a secret a caller
/// guessed right.
///
/// The length check is a deliberate exception: it leaks only the secret's
/// length, which is not the secret, and comparing unequal-length slices has no
/// meaningful definition.
///
/// Lives here because both `gglib-axum` and `gglib-proxy` guard bearer tokens
/// and each had a byte-identical private copy. `normalize_host`, which both
/// guards also call, already lived here. A hardening change to one private copy
/// would not have been reported against the other by any lint.
#[must_use]
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Mint a bearer token for an endpoint that is about to be exposed off
/// loopback.
///
/// A v4 UUID: 122 random bits from the OS CSPRNG, and already the shape this
/// workspace uses for the desktop app's embedded-API token.
#[must_use]
pub fn generate_api_key() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Who may reach the proxy, and how they prove it.
///
/// Two independent gates that happen to travel together, because both are
/// decided at bind time and both are needed by the same layer of the router:
///
/// * [`api_key`](Self::api_key) is opt-in. `None` leaves the endpoint exactly
///   as it behaved before authentication existed.
/// * [`allowed_hosts`](Self::allowed_hosts) is always enforced. It is the
///   DNS-rebinding defence, and it does not depend on a token being set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProxyAccessConfig {
    /// Which origins the CORS layer accepts.
    pub cors: CorsConfig,
    /// Bearer token required on `/v1/*` and `/mcp`. `None` disables the check.
    pub api_key: Option<String>,
    /// Host-header values accepted **in addition to** loopback, normalized to
    /// lowercase with any port stripped. Loopback is always accepted and is
    /// deliberately not listed here — it is a predicate
    /// ([`is_loopback_host`]), so `127.0.0.2` and `::1` are covered without
    /// anyone having to enumerate them.
    pub allowed_hosts: Vec<String>,
}

impl ProxyAccessConfig {
    /// Build the access policy for a proxy about to bind `bind_host`.
    ///
    /// The bound address joins the allowlist automatically when it is a
    /// concrete non-loopback address: someone who asked to bind `192.168.1.5`
    /// plainly intends to be reached at `192.168.1.5`, and making them repeat
    /// it as `--allowed-host` would be a rule with no purpose.
    ///
    /// A wildcard bind (`0.0.0.0` / `::`) gets no such inference. It names no
    /// reachable address, so there is nothing to infer, and guessing the
    /// machine's interface addresses would re-open exactly the hole the
    /// allowlist exists to close. Those deployments must name their hostname
    /// with `--allowed-host`.
    #[must_use]
    pub fn new(
        cors: CorsConfig,
        api_key: Option<String>,
        bind_host: &str,
        extra_hosts: Vec<String>,
    ) -> Self {
        let mut allowed_hosts: Vec<String> = Vec::with_capacity(extra_hosts.len() + 1);

        if !is_loopback_host(bind_host)
            && !is_wildcard_host(bind_host)
            && let Some(host) = normalize_host(bind_host)
        {
            allowed_hosts.push(host);
        }

        for entry in extra_hosts {
            if let Some(host) = normalize_host(&entry)
                && !allowed_hosts.contains(&host)
            {
                allowed_hosts.push(host);
            }
        }

        Self {
            cors,
            api_key,
            allowed_hosts,
        }
    }

    /// Whether a request carrying this `Host` header may proceed.
    ///
    /// This is the DNS-rebinding guard. A rebound page reaches the loopback
    /// socket but still asks for the attacker's hostname, so a `Host` that is
    /// neither loopback nor explicitly allowed did not come from anyone who
    /// knows where this proxy actually lives.
    ///
    /// An absent or unparseable `Host` is rejected: HTTP/1.1 requires the
    /// header, and a request that omits it has no claim to check.
    #[must_use]
    pub fn host_allowed(&self, host_header: &str) -> bool {
        let Some(host) = normalize_host(host_header) else {
            return false;
        };
        // Both sides are already normalized — lowercased, port stripped — so a
        // plain equality comparison is the whole match.
        is_loopback_host(&host) || self.allowed_hosts.contains(&host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The proxy's superset of the two former private copies: it also covers the
    /// empty-vs-nonempty case, which the axum copy did not.
    #[test]
    fn constant_time_eq_agrees_with_equality() {
        assert!(constant_time_eq(b"Bearer abc", b"Bearer abc"));
        assert!(constant_time_eq(b"", b""));
        assert!(!constant_time_eq(b"Bearer abc", b"Bearer abd"));
        assert!(!constant_time_eq(b"Bearer abc", b"Bearer ab"));
        assert!(!constant_time_eq(b"", b"x"));
    }

    /// A prefix match must not pass — the failure mode a naive `starts_with`
    /// would introduce.
    #[test]
    fn constant_time_eq_rejects_a_prefix() {
        assert!(!constant_time_eq(b"Bearer secret", b"Bearer secret-extra"));
    }

    /// The guards read the bare token off this field, so its presence is what
    /// decides whether authentication is enforced at all.
    #[test]
    fn the_api_key_is_carried_verbatim() {
        let off = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "127.0.0.1", vec![]);
        assert_eq!(off.api_key, None);

        let on = ProxyAccessConfig::new(
            CorsConfig::LocalOnly,
            Some("secret123".into()),
            "127.0.0.1",
            vec![],
        );
        assert_eq!(on.api_key.as_deref(), Some("secret123"));
    }

    /// Loopback needs no configuration — the common case must not require a
    /// flag.
    #[test]
    fn loopback_bind_allows_the_usual_local_names() {
        let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "127.0.0.1", vec![]);
        assert!(cfg.allowed_hosts.is_empty());
        for host in [
            "127.0.0.1:8080",
            "localhost:8080",
            "LOCALHOST",
            "[::1]:8080",
            "127.0.0.2",
        ] {
            assert!(cfg.host_allowed(host), "{host} should be allowed");
        }
    }

    /// The whole point: a rebound page asks for its own hostname.
    #[test]
    fn a_foreign_host_is_rejected() {
        let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "127.0.0.1", vec![]);
        assert!(!cfg.host_allowed("evil.com"));
        assert!(!cfg.host_allowed("evil.com:8080"));
        assert!(!cfg.host_allowed(""));
    }

    /// Binding a concrete LAN address is itself the statement that the address
    /// is meant to be reachable.
    #[test]
    fn a_concrete_bind_host_allows_itself() {
        let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "192.168.1.5", vec![]);
        assert_eq!(cfg.allowed_hosts, vec!["192.168.1.5".to_string()]);
        assert!(cfg.host_allowed("192.168.1.5:8080"));
        assert!(cfg.host_allowed("127.0.0.1:8080"));
        assert!(!cfg.host_allowed("evil.com"));
    }

    /// A wildcard names no address, so it grants nothing. This is the
    /// breaking case for existing `--host 0.0.0.0` users, and it is deliberate.
    #[test]
    fn a_wildcard_bind_grants_nothing_on_its_own() {
        let cfg = ProxyAccessConfig::new(CorsConfig::LocalOnly, None, "0.0.0.0", vec![]);
        assert!(cfg.allowed_hosts.is_empty());
        assert!(!cfg.host_allowed("192.168.1.5:8080"));
        assert!(cfg.host_allowed("127.0.0.1:8080"));
    }

    #[test]
    fn explicit_allowed_hosts_are_honoured_under_a_wildcard_bind() {
        let cfg = ProxyAccessConfig::new(
            CorsConfig::LocalOnly,
            None,
            "0.0.0.0",
            vec!["gglib.lan".into(), "192.168.1.5:8080".into()],
        );
        assert!(cfg.host_allowed("gglib.lan"));
        assert!(cfg.host_allowed("GGLIB.LAN:8080"));
        assert!(cfg.host_allowed("192.168.1.5:9999"));
    }

    /// The bind host must not be listed twice when it is also passed
    /// explicitly.
    #[test]
    fn duplicate_entries_collapse() {
        let cfg = ProxyAccessConfig::new(
            CorsConfig::LocalOnly,
            None,
            "192.168.1.5",
            vec!["192.168.1.5".into(), "192.168.1.5:8080".into()],
        );
        assert_eq!(cfg.allowed_hosts, vec!["192.168.1.5".to_string()]);
    }
}
