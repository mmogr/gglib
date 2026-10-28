//! The daemon token at the management API's door.
//!
//! The daemon's socket is the machine's boundary, not the owner's: any account
//! on the machine can reach `127.0.0.1:9887`. Through `/api` it could pair a
//! device, register an MCP server whose command then runs as the owner, or
//! rewrite settings. So [`bearer_guard`] asks every `/api` request for the
//! daemon token, which only the owner's account can read
//! (`gglib_core::access::DaemonToken`), on loopback too. A daemon started
//! `--share-lan` also takes its API key, which the LAN holds. The guard says
//! which of the two admitted a request ([`Admitted`]), for the one route that
//! treats them differently: the reveal of a stored secret hands the key's
//! holder that key and nothing else. `/health` stays outside, so a probe
//! needs nothing.

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gglib_core::access::{
    BearerPolicy, DaemonToken, bearer_matches, daemon_token_path, mint_daemon_token,
};
use gglib_core::contracts::http::daemon::{
    DAEMON_TOKEN_REQUIRED_MESSAGE, DAEMON_TOKEN_REQUIRED_TYPE,
};
use serde_json::json;
use std::sync::Arc;
use tracing::warn;

/// The credentials `/api/*` accepts: the daemon token, and the API key when
/// [`BearerPolicy`] says there is one.
#[derive(Clone)]
pub(crate) struct ApiCredentials {
    pub(crate) policy: BearerPolicy,
    pub(crate) token: Option<DaemonToken>,
}

/// Which credential let a request through [`bearer_guard`], put on the
/// request for a route that treats them differently.
///
/// `POST /api/secrets/reveal` is that route (ADR 0027): a shared daemon's API
/// key is held by clients on the network, so the route hands its holder that
/// key, which the page on the network presents to the proxy, and no other
/// stored secret. No `Debug`, since the key rides along.
#[derive(Clone)]
pub(crate) enum Admitted {
    /// The daemon token, which only the owner's account can read.
    Token,
    /// A `--share-lan` daemon's API key: the one the request presented.
    ApiKey(Arc<str>),
}

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

/// Admit an `/api` request that carries the daemon token, or the API key on a
/// daemon that has one.
///
/// The API key is read from [`BearerPolicy`] rather than frozen at bind, as
/// the proxy's twin guard does: the two doors have to agree about which key
/// is valid right now. The refusal names what was wanted: the key where the
/// daemon has one, since a person may be asked for it, and otherwise the
/// token, with how to get it. A request let through carries [`Admitted`].
pub(crate) async fn bearer_guard(
    State(credentials): State<ApiCredentials>,
    mut req: Request,
    next: Next,
) -> Response {
    let Some(token) = &credentials.token else {
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
    let api_key = credentials.policy.current().await;
    let admitted = if token.admits(presented) {
        Some(Admitted::Token)
    } else {
        api_key
            .clone()
            .filter(|key| bearer_matches(presented, key))
            .map(Admitted::ApiKey)
    };
    if let Some(admitted) = admitted {
        req.extensions_mut().insert(admitted);
        return next.run(req).await;
    }

    warn!(
        path = %req.uri().path(),
        "rejected management API request with a missing or invalid bearer token"
    );
    if api_key.is_some() {
        return refuse(
            StatusCode::UNAUTHORIZED,
            "INVALID_API_KEY",
            "Missing or invalid API key. Send it as 'Authorization: Bearer <key>'.",
        );
    }
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
