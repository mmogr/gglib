//! Access control for the daemon's management API.
//!
//! The management API can start and stop inference, change settings, and
//! queue downloads, so it gets the same two gates the OpenAI proxy received
//! in the `--api-key`/`--allowed-host` work: a Host-header allowlist (the
//! DNS-rebinding guard, always on) and an optional bearer token. The pure
//! policy — normalization, loopback detection, the allowlist itself — is
//! [`gglib_core::ProxyAccessConfig`], shared with the proxy; this module
//! only adapts it to the daemon's router and error shape.
//!
//! One deliberate divergence from the proxy: when the daemon is bound off
//! loopback (`--share-lan`), a `Host` header that is an IP literal is
//! accepted without being listed. DNS rebinding is a hostname attack — a
//! rebound page always presents the attacker's domain, never a bare IP —
//! so refusing `192.168.1.5:9887` would break "reachable by IP" LAN use
//! while stopping nobody. Loopback binds keep the strict policy.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gglib_core::access::{bearer_matches, is_loopback_host, normalize_host};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use serde_json::json;
use tracing::warn;

/// Who may reach the daemon's management API, and how they prove it.
#[derive(Debug, Clone)]
pub struct DaemonAccess {
    /// The shared host-allowlist + bearer policy from `gglib-core`. Its CORS
    /// field is unused here — the daemon router builds CORS separately.
    policy: ProxyAccessConfig,
    /// Accept IP-literal `Host` values that are not on the allowlist. Set
    /// only for non-loopback binds; see the module docs for why this is
    /// safe against rebinding.
    allow_ip_literal_hosts: bool,
}

impl DaemonAccess {
    /// Build the access policy for a daemon about to bind `bind_host`.
    ///
    /// `api_key = None` leaves `/api/*` unauthenticated — the right default
    /// for loopback, where the socket itself is the boundary. Callers that
    /// bind anything else are expected to resolve or mint a key first.
    #[must_use]
    pub fn new(api_key: Option<String>, bind_host: &str, extra_hosts: Vec<String>) -> Self {
        Self {
            policy: ProxyAccessConfig::new(CorsConfig::default(), api_key, bind_host, extra_hosts),
            allow_ip_literal_hosts: !is_loopback_host(bind_host),
        }
    }

    /// The policy for a plain loopback daemon: loopback hosts only, no token.
    #[must_use]
    pub fn loopback() -> Self {
        Self::new(None, "127.0.0.1", Vec::new())
    }

    /// Whether a request carrying this `Host` header may proceed.
    #[must_use]
    pub fn host_allowed(&self, host_header: &str) -> bool {
        if self.policy.host_allowed(host_header) {
            return true;
        }
        self.allow_ip_literal_hosts
            && normalize_host(host_header)
                .is_some_and(|host| host.parse::<std::net::IpAddr>().is_ok())
    }

    /// The bare bearer token this daemon requires, or `None` when
    /// authentication is off.
    ///
    /// The token rather than a pre-formatted `"Bearer <token>"` header,
    /// because [`bearer_matches`] parses the scheme instead of comparing a
    /// fixed string — see the guard below.
    #[must_use]
    pub fn api_key(&self) -> Option<&str> {
        self.policy.api_key.as_deref()
    }
}

/// Reject any request whose `Host` header is not one this daemon answers to.
///
/// Applied as the outermost layer so it covers every route — `/health`, the
/// SPA assets, and paths that match nothing. A check this cheap has no
/// reason to have holes in it.
pub(crate) async fn host_guard(
    State(access): State<Arc<DaemonAccess>>,
    req: Request,
    next: Next,
) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if access.host_allowed(host) {
        return next.run(req).await;
    }

    warn!(
        host,
        path = %req.uri().path(),
        "rejected request with a Host header this daemon does not answer to"
    );
    let remedy = match normalize_host(host) {
        Some(name) => format!(" Add --allowed-host {name} if that is how you reach it."),
        None => String::new(),
    };
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": format!(
                "Host '{host}' is not allowed. The daemon answers to loopback and to hosts \
                 named with --allowed-host.{remedy}"
            ),
            "status": StatusCode::FORBIDDEN.as_u16(),
            "type": "HOST_NOT_ALLOWED",
        })),
    )
        .into_response()
}

/// Require `Authorization: Bearer <token>` before a request reaches `/api/*`.
///
/// Installed only when a token is configured, so the unauthenticated
/// loopback default costs nothing per request. `expected` holds the bare
/// token rather than a pre-formatted `"Bearer <token>"` string: the scheme is
/// matched case-insensitively per RFC 9110, so there is no fixed header to
/// compare against. The proxy's twin guard uses the same
/// [`bearer_matches`] — the two doors have to agree about what a valid
/// credential looks like, and until now they did not.
pub(crate) async fn bearer_guard(
    State(expected): State<Arc<str>>,
    req: Request,
    next: Next,
) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if bearer_matches(presented, &expected) {
        return next.run(req).await;
    }

    warn!(
        path = %req.uri().path(),
        "rejected management API request with a missing or invalid bearer token"
    );
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({
            "error": "Missing or invalid API key. Send it as 'Authorization: Bearer <key>'.",
            "status": StatusCode::UNAUTHORIZED.as_u16(),
            "type": "INVALID_API_KEY",
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Loopback binds keep the proxy's strict policy: no IP-literal
    /// exemption, no foreign hostnames.
    #[test]
    fn loopback_policy_is_strict() {
        let access = DaemonAccess::loopback();
        assert!(access.host_allowed("127.0.0.1:9887"));
        assert!(access.host_allowed("localhost"));
        assert!(!access.host_allowed("192.168.1.5:9887"));
        assert!(!access.host_allowed("evil.com"));
        assert!(!access.host_allowed(""));
    }

    /// A shared daemon must stay reachable by raw IP without ceremony —
    /// that is not a rebinding vector, because a rebound page presents the
    /// attacker's hostname, not an IP.
    #[test]
    fn non_loopback_bind_accepts_ip_literals_but_not_hostnames() {
        let access = DaemonAccess::new(None, "0.0.0.0", vec!["gglib.local".into()]);
        assert!(access.host_allowed("192.168.1.5:9887"));
        assert!(access.host_allowed("[fe80::1]:9887"));
        assert!(access.host_allowed("gglib.local:9887"));
        assert!(access.host_allowed("127.0.0.1:9887"));
        assert!(!access.host_allowed("evil.com:9887"));
    }
}
