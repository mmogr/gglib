//! Access control for the daemon's management API.
//!
//! The management API can start and stop inference, change settings, and
//! queue downloads, so it gets the same two gates the `OpenAI` proxy received
//! in the `--api-key`/`--allowed-host` work: a Host-header allowlist (the
//! DNS-rebinding guard, always on) and a bearer token: the daemon's own,
//! always, and on a `--share-lan` daemon its API key besides. The pure
//! policy — normalization, loopback detection, the allowlist itself — is
//! [`gglib_core::ProxyAccessConfig`], shared with the proxy; this module
//! only adapts it to the daemon's router and error shape.
//!
//! A third gate, [`origin_guard`], refuses a change a browser sends from a
//! page on another site. A page can post to `127.0.0.1:9887` with a loopback
//! `Host` and a body no preflight is asked for. The token refuses such a
//! page, which cannot know it; this gate refuses it as well, and before.
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
    http::{Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gglib_core::access::{BearerPolicy, DaemonToken, is_loopback_host, may_change, normalize_host};
use gglib_core::services::SettingsCache;
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
    /// What every `/api` route asks for; `crate::trust` says why. `None`
    /// shuts `/api` to everybody.
    daemon_token: Option<DaemonToken>,
}

impl DaemonAccess {
    /// Build the access policy for a daemon about to bind `bind_host`.
    ///
    /// `api_key = None` asks no API key of `/api/*` — the right default for
    /// loopback. Callers that bind anything else are expected to resolve or
    /// mint a key first.
    ///
    /// Loopback is the machine's boundary, not the user's, so `/api` asks the
    /// daemon token whatever the key ([`Self::with_daemon_token`]); without
    /// one it serves nothing. A page in a browser that is not the daemon's
    /// own, nor one the router's CORS lets read, changes nothing:
    /// [`origin_guard`] refuses it. `docs/remote.md`, "How it stays private",
    /// says so.
    #[must_use]
    pub fn new(api_key: Option<String>, bind_host: &str, extra_hosts: Vec<String>) -> Self {
        Self {
            policy: ProxyAccessConfig::new(CorsConfig::default(), api_key, bind_host, extra_hosts),
            allow_ip_literal_hosts: !is_loopback_host(bind_host),
            daemon_token: None,
        }
    }

    /// Ask `token` on every `/api` route.
    #[must_use]
    pub fn with_daemon_token(mut self, token: Option<DaemonToken>) -> Self {
        self.daemon_token = token;
        self
    }

    /// The token `/api` asks for, or `None` while it is shut.
    #[must_use]
    pub const fn daemon_token(&self) -> Option<&DaemonToken> {
        self.daemon_token.as_ref()
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

    /// The bare API key this daemon takes beside its token, or `None` when it
    /// takes none.
    ///
    /// The token rather than a pre-formatted `"Bearer <token>"` header,
    /// because [`BearerPolicy`] parses the scheme instead of comparing a
    /// fixed string — see the guard below.
    #[must_use]
    pub fn api_key(&self) -> Option<&str> {
        self.policy.api_key.as_deref()
    }

    /// The bearer policy `/api/*` enforces, decided where the bind host is
    /// still known.
    ///
    /// [`Self::new`] settles whether this daemon takes an API key at all, and
    /// its contract is that `api_key() == None` takes none: only the daemon
    /// token opens `/api/*`. This method is what makes that contract survive
    /// into the router.
    ///
    /// The distinction matters because [`BearerPolicy::tracking`] is not
    /// "enforce this key" — it is "enforce whatever `proxy_api_key` says right
    /// now". That is exactly right for a listener that bound *with* a key,
    /// which must follow a rotation rather than pin the value it started with.
    /// It is wrong for a listener that bound with none: `gglib remote enable`
    /// writes `proxy_api_key` for the *proxy*, and a tracking policy here would
    /// let that unrelated write close the management API — the door the CLI and
    /// the desktop app come through, including the `remote disable` that would
    /// undo it.
    ///
    /// So a keyless daemon gets a policy that names no key, permanently.
    #[must_use]
    #[allow(
        clippy::option_if_let_else,
        reason = "grandfathered at lint inheritance, #1157"
    )]
    pub fn bearer_policy(&self, settings: Arc<SettingsCache>) -> BearerPolicy {
        match self.api_key() {
            Some(key) => BearerPolicy::tracking(Some(key), settings),
            None => BearerPolicy::fixed(None),
        }
    }
}

/// Reject any request whose `Host` header is not one this daemon answers to.
///
/// Applied as the outermost layer so it covers every route — `/health`, the
/// SPA assets, and paths that match nothing. A check this cheap has no
/// reason to have holes in it.
#[allow(
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
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

/// Refuse a change a browser sends from a page on another site.
///
/// [`may_change`] is the policy, asked of everything but `GET`, `HEAD` and
/// `OPTIONS`; `cors` is the config the router's CORS layer answers from, so
/// a page that names any origin but the endpoint's own may change something
/// exactly when the CORS layer lets it read the answer. Sound only where
/// [`host_guard`] runs too, since it is what vouches for the `Host` a
/// same-origin request is matched against.
pub(crate) async fn origin_guard(
    State(cors): State<Arc<CorsConfig>>,
    req: Request,
    next: Next,
) -> Response {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }
    let headers = req.headers();
    // An `Origin` that is not text is refused as `""`, never read as absent.
    let origin = headers
        .get(header::ORIGIN)
        .map(|v| v.to_str().unwrap_or_default());
    let fetch_site = headers.get("sec-fetch-site").and_then(|v| v.to_str().ok());
    let host = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if may_change(&cors, origin, fetch_site, host) {
        return next.run(req).await;
    }

    warn!(
        origin,
        fetch_site,
        path = %req.uri().path(),
        "refused a change sent by a page on another site"
    );
    (
        StatusCode::FORBIDDEN,
        Json(json!({
            "error": "A page on another site may not change anything here. The daemon takes \
                      changes from its own pages, from the origins it lets read its answers, \
                      and from programs, which send no Origin.",
            "status": StatusCode::FORBIDDEN.as_u16(),
            "type": "ORIGIN_NOT_ALLOWED",
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
