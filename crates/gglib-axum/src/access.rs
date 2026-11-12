//! Access control for the daemon's management API.
//!
//! The management API can start and stop inference, change settings, and
//! queue downloads, so it gets the same two gates the `OpenAI` proxy received
//! in the `--api-key`/`--allowed-host` work: a Host-header allowlist (the
//! DNS-rebinding guard, always on) and a bearer token, the daemon's own. The
//! pure policy — normalization, loopback detection, the allowlist itself — is
//! [`gglib_core::ProxyAccessConfig`], shared with the proxy, and the guard
//! that applies it, [`host_guard`], is the one the proxy installs. This
//! module holds what is the daemon's own: [`DaemonAccess`] and, as its
//! [`Endpoint`], the error shape a refusal is written in.
//!
//! A third gate, [`origin_guard`], refuses a change a browser sends from a
//! page on another site. A page can post to `127.0.0.1:9887` with a loopback
//! `Host` and a body no preflight is asked for. The token refuses such a
//! page, which cannot know it; this gate refuses it as well, and before.
//!
//! The daemon always binds `127.0.0.1`, so the allowlist is loopback plus
//! the names `--allowed-host` adds.
//!
//! [`host_guard`]: gglib_proxy::access::host_guard
//! [`origin_guard`]: gglib_proxy::access::origin_guard

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use gglib_core::access::DaemonToken;
use gglib_core::{CorsConfig, ProxyAccessConfig};
use gglib_proxy::access::Endpoint;
use serde_json::json;

/// Who may reach the daemon's management API, and how they prove it.
#[derive(Debug, Clone)]
pub struct DaemonAccess {
    /// The shared host allowlist from `gglib-core`. Its CORS and key fields
    /// are unused here: the daemon router builds CORS separately, and the
    /// daemon takes no API key.
    policy: ProxyAccessConfig,
    /// What every `/api` route asks for; `crate::trust` says why. `None`
    /// shuts `/api` to everybody.
    daemon_token: Option<DaemonToken>,
}

impl DaemonAccess {
    /// Build the access policy for the daemon on `127.0.0.1`, answering to
    /// loopback and to `extra_hosts`.
    ///
    /// Loopback is the machine's boundary, not the user's, so `/api` asks the
    /// daemon token ([`Self::with_daemon_token`]); without one it serves
    /// nothing. A page in a browser that is not the daemon's own, nor one the
    /// router's CORS lets read, changes nothing:
    /// [`origin_guard`](gglib_proxy::access::origin_guard) refuses it.
    /// `docs/remote.md`, "How it stays private", says so.
    #[must_use]
    pub fn new(extra_hosts: Vec<String>) -> Self {
        Self {
            policy: ProxyAccessConfig::new(CorsConfig::default(), None, "127.0.0.1", extra_hosts),
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

    /// The policy for a daemon that answers to loopback hosts only, no token.
    #[must_use]
    pub fn loopback() -> Self {
        Self::new(Vec::new())
    }

    /// Whether a request carrying this `Host` header may proceed.
    #[must_use]
    pub fn host_allowed(&self, host_header: &str) -> bool {
        self.policy.host_allowed(host_header)
    }
}

/// The daemon, to the guards it shares with the proxy: the allowlist above,
/// and the error shape `/api` answers in.
impl Endpoint for DaemonAccess {
    const NAME: &'static str = "daemon";

    fn answers_to(&self, host_header: &str) -> bool {
        self.host_allowed(host_header)
    }

    fn host_refusal(host: &str, remedy: &str) -> Response {
        Json(json!({
            "error": format!(
                "Host '{host}' is not allowed. The daemon answers to loopback and to hosts \
                 named with --allowed-host.{remedy}"
            ),
            "status": StatusCode::FORBIDDEN.as_u16(),
            "type": "HOST_NOT_ALLOWED",
        }))
        .into_response()
    }

    fn origin_refusal() -> Response {
        Json(json!({
            "error": "A page on another site may not change anything here. The daemon takes \
                      changes from its own pages, from the origins it lets read its answers, \
                      and from programs, which send no Origin.",
            "status": StatusCode::FORBIDDEN.as_u16(),
            "type": "ORIGIN_NOT_ALLOWED",
        }))
        .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The daemon answers to loopback and to the names it is given, and to
    /// no other host, an IP literal included.
    #[test]
    fn the_daemon_answers_to_loopback_and_its_named_hosts() {
        let access = DaemonAccess::loopback();
        assert!(access.host_allowed("127.0.0.1:9887"));
        assert!(access.host_allowed("localhost"));
        assert!(!access.host_allowed("192.168.1.5:9887"));
        assert!(!access.host_allowed("evil.com"));
        assert!(!access.host_allowed(""));

        let named = DaemonAccess::new(vec!["gglib.test".into()]);
        assert!(named.host_allowed("gglib.test:9887"));
        assert!(named.host_allowed("127.0.0.1:9887"));
        assert!(!named.host_allowed("192.168.1.5:9887"));
        assert!(!named.host_allowed("evil.com:9887"));
    }
}
