//! What the daemon answers a browser, request by request: the status, the
//! CORS headers and the body, for each way a page or a program can ask.
//!
//! The Host guard, the origin guard and the CORS layer are the ones the proxy
//! installs too (`gglib_proxy::access`). The daemon's own are the envelope a
//! refusal is written in and the order its router layers them in: the Host
//! guard outermost, then CORS over `/api` alone, so a refused `Host` carries
//! no CORS header. `gglib-proxy/tests/integration_browser_guard.rs` asks the
//! proxy the same questions.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, Request};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::origin::{bearer_token, shipped, shipped_cors};
use gglib_axum::{CorsConfig, DaemonAccess};
use gglib_core::contracts::http::daemon;

/// A route that changes nothing, and one that does: a cancel, here of a
/// download that is not queued, which the route answers as done.
const READ: &str = daemon::MODELS_LIST_PATH;
const WRITE: &str = "/api/models/downloads/none/cancel";

/// Stands for the daemon token, which is minted when the tests start.
const TOKEN: (&str, &str) = ("authorization", "the daemon token");
const HOST: (&str, &str) = ("host", "127.0.0.1:9887");
/// A name the daemon is told it answers to, and the page it serves under it.
const NAMED: &str = "gglib.test";
const OWN_HOST: (&str, &str) = ("host", "gglib.test:9887");
const OWN_PAGE: (&str, &str) = ("origin", "http://gglib.test:9887");
const REBOUND: (&str, &str) = ("host", "evil.com:9887");
/// A page the shipped config lets read, and one it does not.
const LOCAL_PAGE: (&str, &str) = ("origin", "http://localhost:5173");
const ELSEWHERE: (&str, &str) = ("origin", "https://evil.example");
/// What a browser says of a request one site sends another.
const CROSS_SITE: (&str, &str) = ("sec-fetch-site", "cross-site");
/// What a preflight asks.
const ASKS_POST: (&str, &str) = ("access-control-request-method", "POST");

/// The CORS layer wraps `/api`, a refusal the origin guard writes included.
const VARY: (&str, &str) = (
    "vary",
    "origin, access-control-request-method, access-control-request-headers",
);
const READS_LOCAL: (&str, &str) = ("access-control-allow-origin", "http://localhost:5173");
const READS_ANY: (&str, &str) = ("access-control-allow-origin", "*");
const ANY_METHOD: (&str, &str) = ("access-control-allow-methods", "*");
const ANY_HEADER: (&str, &str) = ("access-control-allow-headers", "*");

/// What each route answers once the guards have let a request through.
const MODELS: &str = "[]";
const CANCELLED: &str = "";
const ORIGIN_REFUSED: &str = r#"{"error":"A page on another site may not change anything here. The daemon takes changes from its own pages, from the origins it lets read its answers, and from programs, which send no Origin.","status":403,"type":"ORIGIN_NOT_ALLOWED"}"#;
const REBOUND_REFUSED: &str = r#"{"error":"Host 'evil.com:9887' is not allowed. The daemon answers to loopback and to hosts named with --allowed-host. Add --allowed-host evil.com if that is how you reach it.","status":403,"type":"HOST_NOT_ALLOWED"}"#;
/// A `Host` that cannot be one is refused with no flag to suggest.
const NO_HOST_REFUSED: &str = r#"{"error":"Host 'user@evil.com' is not allowed. The daemon answers to loopback and to hosts named with --allowed-host.","status":403,"type":"HOST_NOT_ALLOWED"}"#;

/// One request, and the whole of what the daemon must answer it.
struct Row {
    what: &'static str,
    method: Method,
    path: &'static str,
    headers: &'static [(&'static str, &'static str)],
    status: u16,
    cors: &'static [(&'static str, &'static str)],
    body: &'static str,
}

fn row(
    what: &'static str,
    method: Method,
    path: &'static str,
    headers: &'static [(&'static str, &'static str)],
    status: u16,
    cors: &'static [(&'static str, &'static str)],
    body: &'static str,
) -> Row {
    Row {
        what,
        method,
        path,
        headers,
        status,
        cors,
        body,
    }
}

/// The CORS layer's headers on an answer, sorted by name.
fn cors_headers(headers: &HeaderMap) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = headers
        .iter()
        .filter(|(name, _)| name.as_str().starts_with("access-control-") || *name == "vary")
        .map(|(name, value)| (name.to_string(), value.to_str().unwrap().to_owned()))
        .collect();
    found.sort();
    found
}

/// Ask `app` each row, and hold every answer to it.
async fn ask(app: &Router, rows: &[Row]) {
    let mut wrong = Vec::new();
    for row in rows {
        let mut request = Request::builder().method(row.method.clone()).uri(row.path);
        for (name, value) in row.headers {
            let value = if (*name, *value) == TOKEN {
                HeaderValue::from_str(&bearer_token()).unwrap()
            } else {
                // By bytes, so a row can send an `Origin` that is not text.
                HeaderValue::from_bytes(value.as_bytes()).unwrap()
            };
            request = request.header(HeaderName::from_static(name), value);
        }
        let request = request.body(Body::empty()).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let cors = cors_headers(response.headers());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&bytes).into_owned();
        let expected: Vec<(String, String)> = row
            .cors
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        if (status, &cors, body.as_str()) != (row.status, &expected, row.body) {
            wrong.push(format!("{}: {status} {cors:?} {body}", row.what));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[tokio::test]
async fn each_kind_of_request_gets_its_status_its_cors_headers_and_its_body() {
    let access = DaemonAccess::new(None, "127.0.0.1", vec![NAMED.to_owned()]);
    let app = shipped(&shipped_cors(), access).await;
    #[rustfmt::skip]
    let rows = [
        row("its own page changes something", Method::POST, WRITE, &[TOKEN, OWN_HOST, OWN_PAGE], 200, &[VARY], CANCELLED),
        row("its own page under another name", Method::POST, WRITE, &[TOKEN, HOST, OWN_PAGE], 403, &[VARY], ORIGIN_REFUSED),
        row("a local page changes something", Method::POST, WRITE, &[TOKEN, HOST, LOCAL_PAGE, CROSS_SITE], 200, &[READS_LOCAL, VARY], CANCELLED),
        row("a local page reads", Method::GET, READ, &[TOKEN, HOST, LOCAL_PAGE], 200, &[READS_LOCAL, VARY], MODELS),
        row("another site reads", Method::GET, READ, &[TOKEN, HOST, ELSEWHERE], 200, &[VARY], MODELS),
        row("another site asks for the headers alone", Method::HEAD, READ, &[TOKEN, HOST, ELSEWHERE], 200, &[VARY], ""),
        row("another site changes something", Method::POST, WRITE, &[TOKEN, HOST, ELSEWHERE], 403, &[VARY], ORIGIN_REFUSED),
        row("another site, holding no token", Method::POST, WRITE, &[HOST, ELSEWHERE], 403, &[VARY], ORIGIN_REFUSED),
        row("a page that hides its origin", Method::POST, WRITE, &[TOKEN, HOST, ("origin", "null")], 403, &[VARY], ORIGIN_REFUSED),
        row("an origin that is not text", Method::POST, WRITE, &[TOKEN, HOST, ("origin", "http://\u{e9}vil.example")], 403, &[VARY], ORIGIN_REFUSED),
        row("a program changes something", Method::POST, WRITE, &[TOKEN, HOST], 200, &[VARY], CANCELLED),
        row("a program reads", Method::GET, READ, &[TOKEN, HOST], 200, &[VARY], MODELS),
        row("no origin, but sent across sites", Method::POST, WRITE, &[TOKEN, HOST, CROSS_SITE], 403, &[VARY], ORIGIN_REFUSED),
        row("a rebound name", Method::POST, WRITE, &[TOKEN, REBOUND, ("origin", "http://evil.com:9887")], 403, &[], REBOUND_REFUSED),
        row("a rebound name, from another site", Method::POST, WRITE, &[TOKEN, REBOUND, ELSEWHERE], 403, &[], REBOUND_REFUSED),
        row("a rebound name reads", Method::GET, daemon::HEALTH_PATH, &[REBOUND], 403, &[], REBOUND_REFUSED),
        row("a rebound name, from a local page", Method::GET, READ, &[TOKEN, REBOUND, LOCAL_PAGE], 403, &[], REBOUND_REFUSED),
        row("a host that is no host", Method::GET, READ, &[TOKEN, ("host", "user@evil.com")], 403, &[], NO_HOST_REFUSED),
        row("a preflight from a local page", Method::OPTIONS, WRITE, &[HOST, LOCAL_PAGE, ASKS_POST], 200, &[ANY_HEADER, ANY_METHOD, READS_LOCAL, VARY], ""),
        row("a preflight from another site", Method::OPTIONS, WRITE, &[HOST, ELSEWHERE, ASKS_POST], 200, &[ANY_HEADER, ANY_METHOD, VARY], ""),
        row("a preflight under a rebound name", Method::OPTIONS, WRITE, &[REBOUND, LOCAL_PAGE, ASKS_POST], 403, &[], REBOUND_REFUSED),
    ];
    ask(&app, &rows).await;
}

/// `AllowAll` is the CORS layer's other arm: every page reads, and so every
/// page that says where it is from may change something.
#[tokio::test]
async fn each_kind_of_request_gets_them_under_a_config_that_lets_every_page_read() {
    let app = shipped(&CorsConfig::AllowAll, DaemonAccess::loopback()).await;
    #[rustfmt::skip]
    let rows = [
        row("another site reads", Method::GET, READ, &[TOKEN, HOST, ELSEWHERE], 200, &[READS_ANY, VARY], MODELS),
        row("another site changes something", Method::POST, WRITE, &[TOKEN, HOST, ELSEWHERE], 200, &[READS_ANY, VARY], CANCELLED),
        row("a page that hides its origin", Method::POST, WRITE, &[TOKEN, HOST, ("origin", "null")], 403, &[READS_ANY, VARY], ORIGIN_REFUSED),
        row("a preflight from another site", Method::OPTIONS, WRITE, &[HOST, ELSEWHERE, ASKS_POST], 200, &[ANY_HEADER, ANY_METHOD, READS_ANY, VARY], ""),
    ];
    ask(&app, &rows).await;
}
