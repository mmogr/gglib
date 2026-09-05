#![doc = include_str!("README.md")]
use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gglib_core::ProxyAccessConfig;
use gglib_core::access::bearer_matches;
use tracing::warn;

use crate::models::ErrorResponse;

/// Reject any request whose `Host` header is not one this proxy answers to.
///
/// The DNS-rebinding guard. Applied with [`axum::middleware::from_fn_with_state`]
/// as an outer `layer`, so it covers every route including `/health` and
/// including paths that match nothing — a check this cheap has no reason to
/// have holes in it.
pub(crate) async fn host_guard(
    State(access): State<Arc<ProxyAccessConfig>>,
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
        "rejected request with a Host header this proxy does not answer to"
    );
    // Suggest the normalized name rather than the raw header: `--allowed-host`
    // matches on the host alone, so echoing back `gglib.lan:8080` would teach
    // the reader that the port is part of the value. A header too malformed to
    // normalize gets the generic half of the message and no suggestion.
    let remedy = match gglib_core::access::normalize_host(host) {
        Some(name) => format!(" Add --allowed-host {name} if that is how you reach it."),
        None => String::new(),
    };
    (
        StatusCode::FORBIDDEN,
        Json(ErrorResponse::with_code(
            format!(
                "Host '{host}' is not allowed. This proxy answers to loopback and to hosts \
                 named with --allowed-host.{remedy}"
            ),
            "invalid_request_error",
            "host_not_allowed",
        )),
    )
        .into_response()
}

/// Require `Authorization: Bearer <token>` before a request reaches a handler.
///
/// Applied with `route_layer` so it runs only on matched routes, leaving
/// `/health` — registered outside the protected group — open. The layer is not
/// installed at all when no token is configured, so the unauthenticated default
/// costs nothing per request.
///
/// `expected` holds the bare token rather than a pre-formatted
/// `"Bearer <token>"` string: the scheme is matched case-insensitively per RFC
/// 9110, so there is no fixed header to compare against. See
/// [`bearer_matches`] for what that admits and what it still refuses.
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
        "rejected request with a missing or invalid bearer token"
    );
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(ErrorResponse::with_code(
            "Missing or invalid API key. Send it as 'Authorization: Bearer <key>'.",
            "invalid_request_error",
            "invalid_api_key",
        )),
    )
        .into_response()
}
