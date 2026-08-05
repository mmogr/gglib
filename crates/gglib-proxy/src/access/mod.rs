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
/// `expected` holds the whole `"Bearer <token>"` string rather than the token
/// alone, so the check is one comparison with no per-request formatting.
pub(crate) async fn bearer_guard(
    State(expected): State<Arc<str>>,
    req: Request,
    next: Next,
) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if constant_time_eq(presented.as_bytes(), expected.as_bytes()) {
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

/// Compare two byte strings without an early exit on the first difference.
///
/// A `==` on the token would return as soon as two bytes differ, and the time
/// that takes is a function of how many leading bytes the attacker guessed
/// right — enough, over many requests, to recover the token one byte at a time.
/// Folding every byte into a single accumulator makes the comparison take the
/// same time whatever the input.
///
/// The length check is a deliberate exception: it leaks only the token's
/// length, which is not the secret, and comparing unequal-length slices has no
/// meaningful definition.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
