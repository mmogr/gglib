//! Requests sent the way a page in a browser sends them, for the suites that
//! check the daemon's origin guard.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use gglib_axum::{CorsConfig, DaemonAccess, DaemonOptions};

use super::harness::test_state;

/// The daemon's `Host` on its default loopback bind.
pub(crate) const HOST: &str = "127.0.0.1:9887";
/// A site that is not this machine.
pub(crate) const ELSEWHERE: &str = "https://evil.example";
/// What the CLI sends; the body is then `{}`.
pub(crate) const JSON: (&str, &str) = ("content-type", "application/json");
/// What a form post sends, which no preflight is asked for.
pub(crate) const FORM: (&str, &str) = ("content-type", "text/plain");

/// The CORS config `gglib daemon run` and the desktop app start the daemon
/// with.
pub(crate) fn shipped_cors() -> CorsConfig {
    DaemonOptions::default().cors
}

/// The router a shipped daemon builds, over a context of its own.
pub(crate) async fn shipped(cors: &CorsConfig, access: DaemonAccess) -> Router {
    let state = test_state(cors.clone()).await;
    gglib_axum::create_embedded_spa_router(state, cors, Arc::new(access))
}

/// What came back, read whole.
pub(crate) struct Answer {
    pub(crate) status: StatusCode,
    pub(crate) body: String,
    pub(crate) allow_origin: Option<String>,
}

impl Answer {
    /// Whether the origin guard, and not some other 403, refused it.
    pub(crate) fn refused_for_its_origin(&self) -> bool {
        self.status == StatusCode::FORBIDDEN && self.body.contains("ORIGIN_NOT_ALLOWED")
    }
}

/// Send one request to `host` with `headers`, and read the answer.
pub(crate) async fn send(
    app: &Router,
    method: Method,
    path: &str,
    host: &str,
    headers: &[(&str, &str)],
) -> Answer {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", host);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let body = if headers.contains(&JSON) { "{}" } else { "" };
    send_request(app, request.body(Body::from(body)).unwrap()).await
}

/// Send `request` as it was built, for a header [`send`] cannot spell, and
/// read the answer.
pub(crate) async fn send_request(app: &Router, request: Request<Body>) -> Answer {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let allow_origin = response
        .headers()
        .get("access-control-allow-origin")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&bytes).into_owned();
    Answer {
        status,
        body,
        allow_origin,
    }
}
