#![doc = include_str!("README.md")]
use std::sync::Arc;

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use gglib_core::access::{BearerPolicy, may_change, normalize_host};
use gglib_core::{CorsConfig, ProxyAccessConfig};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tracing::warn;

use crate::models::ErrorResponse;

/// What the Host guard and the origin guard ask of the endpoint behind them.
///
/// Which `Host` it answers to, and the body it refuses in. The proxy says so
/// below and the daemon in `gglib-axum`, and nothing else about
/// [`host_guard`] and [`origin_guard`] differs between the two.
pub trait Endpoint: Send + Sync + 'static {
    /// What a log line calls it.
    const NAME: &'static str;

    /// Whether a request carrying this `Host` header may proceed.
    fn answers_to(&self, host_header: &str) -> bool;

    /// The body of the 403 for a `Host` it does not answer to. `remedy` is
    /// empty, or a sentence, led by a space, naming the flag that admits it.
    fn host_refusal(host: &str, remedy: &str) -> Response;

    /// The body of the 403 for a change a page on another site sent.
    fn origin_refusal() -> Response;
}

/// The proxy: the allowlist as configured, and the `OpenAI` error envelope.
impl Endpoint for ProxyAccessConfig {
    const NAME: &'static str = "proxy";

    fn answers_to(&self, host_header: &str) -> bool {
        self.host_allowed(host_header)
    }

    fn host_refusal(host: &str, remedy: &str) -> Response {
        Json(ErrorResponse::with_code(
            format!(
                "Host '{host}' is not allowed. This proxy answers to loopback and to hosts \
                 named with --allowed-host.{remedy}"
            ),
            "invalid_request_error",
            "host_not_allowed",
        ))
        .into_response()
    }

    fn origin_refusal() -> Response {
        Json(ErrorResponse::with_code(
            "A page on another site may not change anything here. This proxy takes changes \
             from its own origin (one naming the Host the request was sent to), from the \
             origins it lets read its answers, and from programs, which send no Origin.",
            "invalid_request_error",
            "origin_not_allowed",
        ))
        .into_response()
    }
}

/// Reject any request whose `Host` header is not one the endpoint answers to.
///
/// The DNS-rebinding guard. Applied with [`axum::middleware::from_fn_with_state`]
/// as an outer `layer`, so it covers every route including `/health` and
/// including paths that match nothing — a check this cheap has no reason to
/// have holes in it.
#[allow(
    clippy::option_if_let_else,
    reason = "grandfathered at lint inheritance, #1157"
)]
pub async fn host_guard<E: Endpoint>(
    State(endpoint): State<Arc<E>>,
    req: Request,
    next: Next,
) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    if endpoint.answers_to(host) {
        return next.run(req).await;
    }

    warn!(
        host,
        path = %req.uri().path(),
        "rejected request with a Host header this {} does not answer to",
        E::NAME
    );
    // Suggest the normalized name rather than the raw header: `--allowed-host`
    // matches on the host alone, so echoing back `gglib.lan:8080` would teach
    // the reader that the port is part of the value. A header too malformed to
    // normalize gets the generic half of the message and no suggestion.
    let remedy = match normalize_host(host) {
        Some(name) => format!(" Add --allowed-host {name} if that is how you reach it."),
        None => String::new(),
    };
    (StatusCode::FORBIDDEN, E::host_refusal(host, &remedy)).into_response()
}

/// Refuse a change a browser sends from a page on another site.
///
/// [`may_change`] is the policy, asked of everything but `GET`, `HEAD` and
/// `OPTIONS`; `cors` is the config the CORS layer answers from, so a page that
/// names any origin but the endpoint's own may change something exactly when
/// the CORS layer lets it read the answer. A page can post a `text/plain` body
/// to a route that reads raw bytes, or one that reads no body, without a
/// preflight; this is what refuses it. Sound only where [`host_guard`] runs
/// too, since it is what vouches for the `Host` a same-origin request is
/// matched against.
pub async fn origin_guard<E: Endpoint>(
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
    (StatusCode::FORBIDDEN, E::origin_refusal()).into_response()
}

/// Build CORS layer from configuration. It lets an origin read exactly when
/// [`CorsConfig::allows_origin`] does, the test [`origin_guard`] asks too.
pub fn build_cors_layer(config: &CorsConfig) -> CorsLayer {
    let origins = if matches!(config, CorsConfig::AllowAll) {
        AllowOrigin::any()
    } else {
        let config = config.clone();
        AllowOrigin::predicate(move |origin: &HeaderValue, _req_headers| {
            origin
                .to_str()
                .is_ok_and(|origin| config.allows_origin(origin))
        })
    };
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
}

/// Require `Authorization: Bearer <token>` before a request reaches a handler.
///
/// Applied with `route_layer` so it runs only on matched routes, leaving
/// `/health` — registered outside the protected group — open.
///
/// **Installed unconditionally**, so a key set after bind is enforced without
/// a restart. With no key configured the guard costs one cache read and an
/// `Arc` clone per request.
///
/// [`BearerPolicy`] decides which token is required *now* — see its docs for
/// why that is a live question and what the staleness bound is.
pub(crate) async fn bearer_guard(
    State(policy): State<BearerPolicy>,
    req: Request,
    next: Next,
) -> Response {
    let presented = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if policy.admits(presented).await {
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
