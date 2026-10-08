//! The daemon and the proxy stand behind one Host guard, one origin guard and
//! one CORS layer (`gglib_proxy::access`), each as an `Endpoint`. Put behind
//! the same layers over the same route, the two answer every request alike:
//! the same status, the same CORS headers and, once the guards let a request
//! through, the same body. A refusal's body is the one thing each writes its
//! own way, and both name the same code in it.
//!
//! `daemon_browser_guard.rs` and the proxy's `integration_browser_guard.rs`
//! hold each real router to its answers; this holds the two to each other.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request};
use axum::middleware::from_fn_with_state;
use axum::routing::get;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use gglib_axum::DaemonAccess;
use gglib_core::{CorsConfig, ProxyAccessConfig};
use gglib_proxy::access::{Endpoint, build_cors_layer, host_guard, origin_guard};

const HOST: (&str, &str) = ("host", "127.0.0.1:9887");
const REBOUND: (&str, &str) = ("host", "evil.com:9887");
const LOCAL_PAGE: (&str, &str) = ("origin", "http://localhost:5173");
const ELSEWHERE: (&str, &str) = ("origin", "https://evil.example");

/// A method, the headers it is sent with, and the code both must refuse it
/// with, or `None` where both must let it through.
type Asked = (
    &'static str,
    &'static [(&'static str, &'static str)],
    Option<&'static str>,
);

const HOST_REFUSED: Option<&str> = Some("host_not_allowed");
const ORIGIN_REFUSED: Option<&str> = Some("origin_not_allowed");

#[rustfmt::skip]
const REQUESTS: &[Asked] = &[
    ("GET", &[HOST], None),
    ("POST", &[HOST], None),
    ("POST", &[HOST, ("origin", "http://127.0.0.1:9887")], None),
    ("POST", &[HOST, LOCAL_PAGE], None),
    ("GET", &[HOST, ELSEWHERE], None),
    ("POST", &[HOST, ELSEWHERE], ORIGIN_REFUSED),
    ("POST", &[HOST, ("origin", "null")], ORIGIN_REFUSED),
    ("POST", &[HOST, ("sec-fetch-site", "cross-site")], ORIGIN_REFUSED),
    ("POST", &[REBOUND, ("origin", "http://evil.com:9887")], HOST_REFUSED),
    ("POST", &[REBOUND, ELSEWHERE], HOST_REFUSED),
    ("GET", &[REBOUND, LOCAL_PAGE], HOST_REFUSED),
    ("GET", &[("host", "user@evil.com")], HOST_REFUSED),
    ("OPTIONS", &[HOST, LOCAL_PAGE, ("access-control-request-method", "POST")], None),
];

/// One route behind the three layers, CORS outermost.
fn guarded<E: Endpoint>(endpoint: E, cors: &CorsConfig) -> Router {
    Router::new()
        .route("/", get(|| async { "read" }).post(|| async { "changed" }))
        .layer(from_fn_with_state(
            Arc::new(cors.clone()),
            origin_guard::<E>,
        ))
        .layer(from_fn_with_state(Arc::new(endpoint), host_guard::<E>))
        .layer(build_cors_layer(cors))
}

/// The status, the headers and the body `app` answers one request with.
async fn answer(app: &Router, (method, headers, _): &Asked) -> (u16, HeaderMap, String) {
    let mut request = Request::builder().method(*method).uri("/");
    for (name, value) in *headers {
        request = request.header(*name, *value);
    }
    let request = request.body(Body::empty()).unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        headers,
        String::from_utf8_lossy(&bytes).into_owned(),
    )
}

#[tokio::test]
async fn the_daemon_and_the_proxy_differ_only_in_the_body_of_a_refusal() {
    for cors in [CorsConfig::LocalOnly, CorsConfig::AllowAll] {
        let daemon = guarded(DaemonAccess::loopback(), &cors);
        let proxy = guarded(ProxyAccessConfig::default(), &cors);
        for asked in REQUESTS {
            let (daemon_status, mut daemon_headers, daemon_body) = answer(&daemon, asked).await;
            let (proxy_status, mut proxy_headers, proxy_body) = answer(&proxy, asked).await;
            // `AllowAll` lets the page on another site change something.
            let elsewhere_writes = asked.0 == "POST" && asked.1 == [HOST, ELSEWHERE];
            let refused = if cors == CorsConfig::AllowAll && elsewhere_writes {
                None
            } else {
                asked.2
            };
            let Some(code) = refused else {
                assert_eq!(
                    (daemon_status, proxy_status),
                    (200, 200),
                    "{cors:?} {asked:?}"
                );
                assert_eq!(daemon_headers, proxy_headers, "{cors:?} {asked:?}");
                assert_eq!(daemon_body, proxy_body, "{cors:?} {asked:?}");
                continue;
            };

            assert_eq!(
                (daemon_status, proxy_status),
                (403, 403),
                "{cors:?} {asked:?}"
            );
            // The two bodies are not one length, and nothing else differs.
            daemon_headers.remove("content-length");
            proxy_headers.remove("content-length");
            assert_eq!(daemon_headers, proxy_headers, "{cors:?} {asked:?}");
            let proxy_body: Value = serde_json::from_str(&proxy_body).unwrap();
            let daemon_body: Value = serde_json::from_str(&daemon_body).unwrap();
            let in_the_proxys = json!({ "error": {
                "message": proxy_body["error"]["message"].as_str().unwrap(),
                "type": "invalid_request_error",
                "code": code,
            }});
            let in_the_daemons = json!({
                "error": daemon_body["error"].as_str().unwrap(),
                "status": 403,
                "type": code.to_uppercase(),
            });
            assert_eq!(proxy_body, in_the_proxys, "{cors:?} {asked:?}");
            assert_eq!(daemon_body, in_the_daemons, "{cors:?} {asked:?}");
        }
    }
}

/// Both routers put the CORS layer outside the origin guard, and that layer
/// answers every `OPTIONS` itself. The guard judges none even so: one that
/// reaches it from another site goes on to the route, which here has no such
/// method.
#[tokio::test]
async fn the_origin_guard_lets_an_options_request_from_another_site_through() {
    let app = Router::new()
        .route("/", get(|| async { "read" }))
        .layer(from_fn_with_state(
            Arc::new(CorsConfig::LocalOnly),
            origin_guard::<DaemonAccess>,
        ));
    let (status, _, _) = answer(&app, &("OPTIONS", &[HOST, ELSEWHERE], None)).await;
    assert_eq!(status, 405);
}
