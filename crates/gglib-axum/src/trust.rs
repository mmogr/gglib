//! The daemon token at the management API's door.
//!
//! A loopback daemon asks no API key: the socket is the boundary. It is the
//! machine's boundary, though, not the owner's, and a few routes hand whoever
//! reaches them something that outlives them: enabling the tunnel, inviting a
//! device, forgetting one, joining another machine. [`trust_guard`] stands in
//! front of those and admits only the daemon token, which only the owner's
//! account can read (`gglib_core::access::DaemonToken`). The API key cannot
//! stand in for it: the settings route returns that key to anybody who asks,
//! and on a `--share-lan` daemon it is the key the LAN holds.
//!
//! [`bearer_guard`] is the outer door, in front of every `/api/*` route. It
//! admits the daemon token beside the API key, so a client that holds only
//! the token, as `gglib` on this machine and the page `gglib web` opens do,
//! gets through both.

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gglib_core::access::{BearerPolicy, DaemonToken, daemon_token_path, load_or_mint_daemon_token};
use gglib_core::contracts::http::daemon::{
    DAEMON_TOKEN_REQUIRED_MESSAGE, DAEMON_TOKEN_REQUIRED_TYPE,
};
use serde_json::json;
use tracing::warn;

/// The credentials `/api/*` accepts: the API key as [`BearerPolicy`] says,
/// and the daemon token.
#[derive(Clone)]
pub(crate) struct ApiCredentials {
    pub(crate) policy: BearerPolicy,
    pub(crate) token: Option<DaemonToken>,
}

/// The daemon's token, read from its file or minted into it.
///
/// A token that cannot be had is logged and leaves the trust routes shut to
/// everybody rather than stopping the daemon: every other route still works,
/// and the log says why pairing does not.
pub(crate) fn daemon_token() -> Option<DaemonToken> {
    let loaded = daemon_token_path()
        .map_err(|e| std::io::Error::other(e.to_string()))
        .and_then(|path| load_or_mint_daemon_token(&path));
    match loaded {
        Ok(token) => Some(token),
        Err(e) => {
            warn!("no daemon token, so the routes that change who is trusted are shut: {e}");
            None
        }
    }
}

/// The raw `Authorization` header, or `None` when there is none or it is
/// not text.
fn presented(req: &Request) -> Option<&str> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
}

/// Require `Authorization: Bearer <token>` before a request reaches `/api/*`,
/// where the token is the API key or the daemon token.
///
/// Installed unconditionally, and reading the API key it requires from
/// [`BearerPolicy`] rather than from a value frozen at bind. The proxy's twin
/// guard uses the same type for the same reasons: the two doors have to agree
/// about what a valid credential looks like, and about which credential is
/// valid right now.
pub(crate) async fn bearer_guard(
    State(credentials): State<ApiCredentials>,
    req: Request,
    next: Next,
) -> Response {
    let presented = presented(&req);
    let holds_token = credentials
        .token
        .as_ref()
        .is_some_and(|token| token.admits(presented));
    if holds_token || credentials.policy.admits(presented).await {
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

/// Admit a request to a route that changes who is trusted only when it
/// carries the daemon token, on loopback too. Installed with `route_layer`
/// on those routes alone.
pub(crate) async fn trust_guard(
    State(token): State<Option<DaemonToken>>,
    req: Request,
    next: Next,
) -> Response {
    if token
        .as_ref()
        .is_some_and(|token| token.admits(presented(&req)))
    {
        return next.run(req).await;
    }

    warn!(
        path = %req.uri().path(),
        "refused a route that changes who is trusted: no daemon token"
    );
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({
            "error": DAEMON_TOKEN_REQUIRED_MESSAGE,
            "status": StatusCode::UNAUTHORIZED.as_u16(),
            "type": DAEMON_TOKEN_REQUIRED_TYPE,
        })),
    )
        .into_response()
}
