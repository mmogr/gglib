//! The daemon token at the management API's door.
//!
//! The daemon's socket is the machine's boundary, not the owner's: any account
//! on the machine can reach `127.0.0.1:9887`. Through `/api` it could pair a
//! device, register an MCP server whose command then runs as the owner, or
//! rewrite settings. So [`bearer_guard`] asks every `/api` request for the
//! daemon token, which only the owner's account can read
//! (`gglib_core::access::DaemonToken`), on loopback too. No other credential
//! opens it: the proxy's API key is the proxy's alone. `/health` stays
//! outside, so a probe needs nothing.

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gglib_core::access::{DaemonToken, daemon_token_path, mint_daemon_token};
use gglib_core::contracts::http::daemon::{
    DAEMON_TOKEN_REQUIRED_MESSAGE, DAEMON_TOKEN_REQUIRED_TYPE,
};
use serde_json::json;
use tracing::warn;

/// A new token for this start, written over the last one.
///
/// A token that cannot be had is logged, and `/api` then serves nothing: a
/// door with no key to ask for would otherwise stand open.
pub(crate) fn daemon_token() -> Option<DaemonToken> {
    let minted = daemon_token_path()
        .map_err(|e| std::io::Error::other(e.to_string()))
        .and_then(|path| mint_daemon_token(&path));
    match minted {
        Ok(token) => Some(token),
        Err(e) => {
            warn!("no daemon token, so /api serves nothing this run: {e}");
            None
        }
    }
}

/// Admit an `/api` request that carries the daemon token. The refusal names
/// the token and how to get it.
pub(crate) async fn bearer_guard(
    State(token): State<Option<DaemonToken>>,
    req: Request,
    next: Next,
) -> Response {
    let Some(token) = &token else {
        return refuse(
            StatusCode::SERVICE_UNAVAILABLE,
            "DAEMON_TOKEN_MISSING",
            "The daemon could not make its token, so /api serves nothing this run; \
             its log says why.",
        );
    };
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if token.admits(presented) {
        return next.run(req).await;
    }

    warn!(
        path = %req.uri().path(),
        "rejected management API request with a missing or invalid bearer token"
    );
    refuse(
        StatusCode::UNAUTHORIZED,
        DAEMON_TOKEN_REQUIRED_TYPE,
        DAEMON_TOKEN_REQUIRED_MESSAGE,
    )
}

/// The daemon's error shape, with `WWW-Authenticate` on a 401.
fn refuse(status: StatusCode, kind: &str, message: &str) -> Response {
    let body = Json(json!({
        "error": message,
        "status": status.as_u16(),
        "type": kind,
    }));
    if status == StatusCode::UNAUTHORIZED {
        let challenge = [(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"))];
        return (status, challenge, body).into_response();
    }
    (status, body).into_response()
}
