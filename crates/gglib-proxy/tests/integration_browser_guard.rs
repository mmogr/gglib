//! What the proxy answers a browser, request by request: the status, the CORS
//! headers and the body, for each way a page or a program can ask.
//!
//! The Host guard, the origin guard and the CORS layer are the ones the daemon
//! installs too (`gglib_proxy::access`). The proxy's own are the envelope a
//! refusal is written in and the order its router layers them in, CORS
//! outermost, so a refusal still carries the CORS layer's headers.
//! `gglib-axum/tests/daemon_browser_guard.rs` asks the daemon the same
//! questions.

use gglib_core::{CorsConfig, ProxyAccessConfig};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};

mod fixtures;
use fixtures::access::spawn_proxy;

/// A route that changes nothing, and one that does.
const READ: &str = "/health";
const WRITE: &str = "/v1/proxy/cache/clear";

/// A name the proxy is told it answers to, and the page it serves under it.
const NAMED: &str = "gglib.test";
const OWN_HOST: (&str, &str) = ("host", "gglib.test:8080");
const OWN_PAGE: (&str, &str) = ("origin", "http://gglib.test:8080");
const REBOUND: (&str, &str) = ("host", "evil.com:8080");
/// A page `LocalOnly` lets read, and one it does not.
const LOCAL_PAGE: (&str, &str) = ("origin", "http://localhost:5173");
const ELSEWHERE: (&str, &str) = ("origin", "https://evil.example");
/// What a browser says of a request one site sends another.
const CROSS_SITE: (&str, &str) = ("sec-fetch-site", "cross-site");
/// What a preflight asks.
const ASKS_POST: (&str, &str) = ("access-control-request-method", "POST");

/// The CORS layer wraps every answer, a refusal included.
const VARY: (&str, &str) = (
    "vary",
    "origin, access-control-request-method, access-control-request-headers",
);
const READS_LOCAL: (&str, &str) = ("access-control-allow-origin", "http://localhost:5173");
const READS_ANY: (&str, &str) = ("access-control-allow-origin", "*");
const ANY_METHOD: (&str, &str) = ("access-control-allow-methods", "*");
const ANY_HEADER: (&str, &str) = ("access-control-allow-headers", "*");

/// What each route answers once the guards have let a request through.
const HEALTHY: &str = r#"{"status":"ok"}"#;
const CLEARED: &str = r#"{"disk":"disk cache not enabled","message":"disk cache not enabled; model recycled, RAM cache flushed","ram":"model recycled, RAM cache flushed","status":"ok"}"#;
const ORIGIN_REFUSED: &str = r#"{"error":{"message":"A page on another site may not change anything here. This proxy takes changes from its own origin (one naming the Host the request was sent to), from the origins it lets read its answers, and from programs, which send no Origin.","type":"invalid_request_error","code":"origin_not_allowed"}}"#;
const REBOUND_REFUSED: &str = r#"{"error":{"message":"Host 'evil.com:8080' is not allowed. This proxy answers to loopback and to hosts named with --allowed-host. Add --allowed-host evil.com if that is how you reach it.","type":"invalid_request_error","code":"host_not_allowed"}}"#;
/// A `Host` that cannot be one is refused with no flag to suggest.
const NO_HOST_REFUSED: &str = r#"{"error":{"message":"Host 'user@evil.com' is not allowed. This proxy answers to loopback and to hosts named with --allowed-host.","type":"invalid_request_error","code":"host_not_allowed"}}"#;

/// One request, and the whole of what the proxy must answer it.
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

/// Ask the proxy at `base` each row, and hold every answer to it.
async fn ask(base: &str, rows: &[Row]) {
    let client = Client::new();
    let mut wrong = Vec::new();
    for row in rows {
        let mut request = client.request(row.method.clone(), format!("{base}{}", row.path));
        for (name, value) in row.headers {
            // By bytes, so a row can send an `Origin` that is not text.
            let value = HeaderValue::from_bytes(value.as_bytes()).unwrap();
            request = request.header(HeaderName::from_static(name), value);
        }
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let cors = cors_headers(response.headers());
        let body = response.text().await.unwrap();
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
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        None,
        "127.0.0.1",
        vec![NAMED.to_owned()],
    );
    let (base, _port, cancel) = spawn_proxy(access).await;
    #[rustfmt::skip]
    let rows = [
        row("its own page changes something", Method::POST, WRITE, &[OWN_HOST, OWN_PAGE], 200, &[VARY], CLEARED),
        row("its own page under another name", Method::POST, WRITE, &[OWN_PAGE], 403, &[VARY], ORIGIN_REFUSED),
        row("a local page changes something", Method::POST, WRITE, &[LOCAL_PAGE, CROSS_SITE], 200, &[READS_LOCAL, VARY], CLEARED),
        row("a local page reads", Method::GET, READ, &[LOCAL_PAGE], 200, &[READS_LOCAL, VARY], HEALTHY),
        row("another site reads", Method::GET, READ, &[ELSEWHERE], 200, &[VARY], HEALTHY),
        row("another site asks for the headers alone", Method::HEAD, READ, &[ELSEWHERE], 200, &[VARY], ""),
        row("another site changes something", Method::POST, WRITE, &[ELSEWHERE], 403, &[VARY], ORIGIN_REFUSED),
        row("a page that hides its origin", Method::POST, WRITE, &[("origin", "null")], 403, &[VARY], ORIGIN_REFUSED),
        row("an origin that is not text", Method::POST, WRITE, &[("origin", "http://\u{e9}vil.example")], 403, &[VARY], ORIGIN_REFUSED),
        row("a program changes something", Method::POST, WRITE, &[], 200, &[VARY], CLEARED),
        row("a program reads", Method::GET, READ, &[], 200, &[VARY], HEALTHY),
        row("no origin, but sent across sites", Method::POST, WRITE, &[CROSS_SITE], 403, &[VARY], ORIGIN_REFUSED),
        row("a rebound name", Method::POST, WRITE, &[REBOUND, ("origin", "http://evil.com:8080")], 403, &[VARY], REBOUND_REFUSED),
        row("a rebound name, from another site", Method::POST, WRITE, &[REBOUND, ELSEWHERE], 403, &[VARY], REBOUND_REFUSED),
        row("a rebound name reads", Method::GET, READ, &[REBOUND], 403, &[VARY], REBOUND_REFUSED),
        row("a rebound name, from a local page", Method::GET, READ, &[REBOUND, LOCAL_PAGE], 403, &[READS_LOCAL, VARY], REBOUND_REFUSED),
        row("a host that is no host", Method::GET, READ, &[("host", "user@evil.com")], 403, &[VARY], NO_HOST_REFUSED),
        row("a preflight from a local page", Method::OPTIONS, WRITE, &[LOCAL_PAGE, ASKS_POST], 200, &[ANY_HEADER, ANY_METHOD, READS_LOCAL, VARY], ""),
        row("a preflight from another site", Method::OPTIONS, WRITE, &[ELSEWHERE, ASKS_POST], 200, &[ANY_HEADER, ANY_METHOD, VARY], ""),
        row("a preflight under a rebound name", Method::OPTIONS, WRITE, &[REBOUND, LOCAL_PAGE, ASKS_POST], 200, &[ANY_HEADER, ANY_METHOD, READS_LOCAL, VARY], ""),
    ];
    ask(&base, &rows).await;
    cancel.cancel();
}

/// `AllowAll` is the CORS layer's other arm: every page reads, and so every
/// page that says where it is from may change something.
#[tokio::test]
async fn each_kind_of_request_gets_them_under_a_config_that_lets_every_page_read() {
    let access = ProxyAccessConfig {
        cors: CorsConfig::AllowAll,
        ..ProxyAccessConfig::default()
    };
    let (base, _port, cancel) = spawn_proxy(access).await;
    #[rustfmt::skip]
    let rows = [
        row("another site reads", Method::GET, READ, &[ELSEWHERE], 200, &[READS_ANY, VARY], HEALTHY),
        row("another site changes something", Method::POST, WRITE, &[ELSEWHERE], 200, &[READS_ANY, VARY], CLEARED),
        row("a page that hides its origin", Method::POST, WRITE, &[("origin", "null")], 403, &[READS_ANY, VARY], ORIGIN_REFUSED),
        row("a preflight from another site", Method::OPTIONS, WRITE, &[ELSEWHERE, ASKS_POST], 200, &[ANY_HEADER, ANY_METHOD, READS_ANY, VARY], ""),
    ];
    ask(&base, &rows).await;
    cancel.cancel();
}
