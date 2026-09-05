#![doc = include_str!("README.md")]
mod bearer;
mod host;

#[cfg(test)]
mod access_tests;
#[cfg(test)]
mod host_tests;

pub use bearer::{BearerPolicy, bearer_matches};
pub use host::{is_loopback_host, is_wildcard_host, normalize_host};

use std::sync::Arc;

use crate::cors::CorsConfig;
use crate::ports::RemoteGatewayPort;

/// Where the proxy's bearer token came from.
///
/// Carried alongside the resolved value so the startup banner can explain the
/// decision rather than merely stating it — the same `(value, source)` shape
/// [`resolve_context_size_with_source`](crate::server_config::resolve_context_size_with_source)
/// uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApiKeySource {
    /// Supplied on the command line or through `GGLIB_API_KEY`.
    Flag,
    /// Read from the stored `proxy_api_key` setting.
    Settings,
    /// Minted by this run because the bind was not loopback and nothing else
    /// supplied one. Printed once, then persisted to settings.
    Generated,
    /// No token configured; the endpoint is unauthenticated.
    #[default]
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

/// Mint a six-digit pairing code for `gglib remote enable`.
///
/// Zero-padded decimal, so it is something a person reads off one screen
/// and types into another. Its entropy is deliberately small — about twenty
/// bits — and it is not a credential on its own: it lives two minutes, is
/// spent on first use, is burned after three wrong attempts, and reaching
/// the route that accepts it requires the ticket. ADR 0012 has the
/// argument. Drawn from the same CSPRNG as [`generate_api_key`]; the modulo
/// bias over a 128-bit draw is not measurable.
#[must_use]
pub fn generate_pairing_code() -> String {
    let draw = uuid::Uuid::new_v4().as_u128();
    // `as` would be a lint here; the remainder is by construction below
    // one million and fits comfortably.
    let six = u32::try_from(draw % 1_000_000).unwrap_or(0);
    format!("{six:06}")
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
#[derive(Debug, Clone, Default)]
pub struct ProxyAccessConfig {
    /// Which origins the CORS layer accepts.
    pub cors: CorsConfig,
    /// Bearer token required on `/v1/*` and `/mcp`. `None` disables the check.
    pub api_key: Option<String>,
    /// Where [`api_key`](Self::api_key) came from, which decides whether it may
    /// later be replaced by a settings write. A flag or environment value
    /// outranks the stored setting, so it must not be overridden by one; every
    /// other source is the stored setting, or absent, and tracks it.
    pub api_key_source: ApiKeySource,
    /// Host-header values accepted **in addition to** loopback, normalized to
    /// lowercase with any port stripped. Loopback is always accepted and is
    /// deliberately not listed here — it is a predicate
    /// ([`is_loopback_host`]), so `127.0.0.2` and `::1` are covered without
    /// anyone having to enumerate them.
    pub allowed_hosts: Vec<String>,
    /// The remote tunnel's owner, when this proxy may be reached through one
    /// (ADR 0012). Travels with the access policy because it *is* one: it
    /// redeems the pairing code that hands out the key, and it decides
    /// whether a request that arrived through the tunnel may reach `/mcp`.
    /// `None` for an embedded server or a test, where nothing is listening
    /// for the answers.
    pub remote: Option<Arc<dyn RemoteGatewayPort>>,
}

/// Equality is over the *policy* — CORS, token, source, hosts — and not
/// over [`remote`](ProxyAccessConfig::remote), which is a live object rather
/// than a value. Two configs that differ only in whether a tunnel owner is
/// attached describe the same access rules.
impl PartialEq for ProxyAccessConfig {
    fn eq(&self, other: &Self) -> bool {
        self.cors == other.cors
            && self.api_key == other.api_key
            && self.api_key_source == other.api_key_source
            && self.allowed_hosts == other.allowed_hosts
    }
}

impl Eq for ProxyAccessConfig {}

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
            api_key_source: ApiKeySource::default(),
            allowed_hosts,
            remote: None,
        }
    }

    /// Attach the remote tunnel's owner.
    ///
    /// Separate from [`new`](Self::new) for the reason
    /// [`with_key_source`](Self::with_key_source) is: only the supervisor has
    /// one to attach, and every other construction site means "no tunnel".
    #[must_use]
    pub fn with_remote(mut self, remote: Option<Arc<dyn RemoteGatewayPort>>) -> Self {
        self.remote = remote;
        self
    }

    /// Record where the token came from.
    ///
    /// Separate from [`new`](Self::new) so that adding it did not change a
    /// signature every caller spells out; the supervisor is the only layer
    /// that knows the answer, and every other construction site means
    /// [`ApiKeySource::None`].
    #[must_use]
    pub const fn with_key_source(mut self, source: ApiKeySource) -> Self {
        self.api_key_source = source;
        self
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
