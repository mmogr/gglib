//! Every answer the proxy's router gives that is not a success is `OpenAI`'s
//! error envelope, `{"error": {"message", "type", "code"}}`, with a code from
//! `docs/error-codes.json` and the type and status the file gives it.
//!
//! One request a case, each one of the answers no handler writes for itself:
//! a path no route serves, a method a route does not take, a body, query or
//! path segment an extractor could not read, a body over its route's limit,
//! each guard, and the administrative routes. Left to axum, the first few
//! were plain text or an empty body.

use std::collections::BTreeMap;

use gglib_core::contracts::http::MAX_BODY_BYTES;
use gglib_core::{CorsConfig, ProxyAccessConfig};
use gglib_proxy::ServeConfig;
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde_json::Value;

mod fixtures;
use fixtures::spawn::{defaults, spawn};

const KEY: &str = "sk-zzq-error-shape";

/// What a case sends as its body.
enum Body {
    None,
    /// Text under a content type.
    Text(&'static str, &'static str),
    /// A JSON object of this many bytes, sent as JSON.
    Sized(usize),
}

struct Case {
    what: &'static str,
    method: Method,
    path: &'static str,
    /// Whether the request carries the key.
    bearer: bool,
    headers: &'static [(&'static str, &'static str)],
    body: Body,
    status: u16,
    code: &'static str,
}

/// One row: what is sent, then the status and code it is answered with.
const fn case(
    what: &'static str,
    method: Method,
    path: &'static str,
    bearer: bool,
    headers: &'static [(&'static str, &'static str)],
    body: Body,
    (status, code): (u16, &'static str),
) -> Case {
    Case {
        what,
        method,
        path,
        bearer,
        headers,
        body,
        status,
        code,
    }
}

const JSON: &str = "application/json";
const TUNNELLED: (&str, &str) = ("via", "1.1 modelpipe");
const DEVICE: (&str, &str) = ("x-modelpipe-device", "dev-0a1b2c3d");

/// The table: what to send, and the status and code it is answered with.
#[rustfmt::skip]
fn cases() -> Vec<Case> {
    use Body::{None as Empty, Sized, Text};
    const MIB: usize = 1024 * 1024;
    vec![
        // What no route answers.
        case("a path no route serves", Method::GET, "/v1/nope", true, &[], Empty, (404, "unknown_route")),
        case("a route from before /v1/gglib", Method::GET, "/v1/proxy/status", true, &[], Empty, (404, "unknown_route")),
        case("a method a guarded route does not take", Method::DELETE, "/v1/models", true, &[], Empty, (405, "method_not_allowed")),
        case("a method a route with a body limit does not take", Method::GET, "/v1/chat/completions", true, &[], Empty, (405, "method_not_allowed")),
        case("a method /health does not take", Method::POST, "/health", false, &[], Empty, (405, "method_not_allowed")),
        case("a method /mcp does not take", Method::PUT, "/mcp", true, &[], Empty, (405, "method_not_allowed")),
        // The guards.
        case("no key", Method::GET, "/v1/models", false, &[], Empty, (401, "invalid_api_key")),
        case("a Host not allowed", Method::GET, "/v1/models", true, &[("host", "evil.com:8080")], Empty, (403, "host_not_allowed")),
        case("a change from another site", Method::POST, "/v1/chat/completions", true, &[("origin", "https://evil.example")], Text(JSON, "{}"), (403, "origin_not_allowed")),
        case("a tunnelled request that names no device", Method::GET, "/v1/models", true, &[TUNNELLED], Empty, (403, "device_not_paired")),
        case("a tunnelled request to /mcp", Method::POST, "/mcp", true, &[TUNNELLED, DEVICE], Text(JSON, "{}"), (403, "mcp_not_allowed_over_tunnel")),
        case("the hub's chats asked locally", Method::GET, "/v1/gglib/chats", true, &[], Empty, (403, "device_not_named")),
        // A wrong method meets the route's guards first, as its right one would.
        case("a wrong method with no key", Method::DELETE, "/v1/models", false, &[], Empty, (401, "invalid_api_key")),
        case("a wrong method tunnelled, naming no device", Method::DELETE, "/v1/models", true, &[TUNNELLED], Empty, (403, "device_not_paired")),
        case("a wrong method on /mcp, tunnelled", Method::PUT, "/mcp", true, &[TUNNELLED, DEVICE], Empty, (403, "mcp_not_allowed_over_tunnel")),
        case("a wrong method on the hub's chats, locally", Method::DELETE, "/v1/gglib/chats", true, &[], Empty, (403, "device_not_named")),
        // What an extractor could not read, or a body over its limit.
        case("a chat body that is not JSON", Method::POST, "/v1/chat/completions", true, &[], Text(JSON, "{not json"), (400, "invalid_request")),
        case("a session id that is not one", Method::POST, "/v1/chat/completions", true, &[("x-gglib-session-id", "../up")], Text(JSON, r#"{"model":"m","messages":[]}"#), (400, "invalid_request")),
        case("a chat body over 32 MiB", Method::POST, "/v1/chat/completions", true, &[], Sized(MAX_BODY_BYTES + 1), (413, "request_too_large")),
        case("an embeddings body over 2 MiB", Method::POST, "/v1/embeddings", true, &[], Sized(2 * MIB + 1), (413, "request_too_large")),
        case("a path segment that is not UTF-8", Method::GET, "/v1/models/%FF", true, &[], Empty, (400, "invalid_request")),
        case("a drawing query that does not read", Method::GET, "/v1/gglib/images/drawing?far=maybe", true, &[], Empty, (400, "invalid_request")),
        case("a load body not sent as JSON", Method::POST, "/v1/gglib/models/m/load", true, &[], Text("text/plain", "{}"), (400, "invalid_request")),
        case("a load body of the wrong shape", Method::POST, "/v1/gglib/models/m/load", true, &[], Text(JSON, r#"{"num_ctx":"many"}"#), (400, "invalid_request")),
        // The administrative routes.
        case("a shutdown body not sent as JSON", Method::POST, "/v1/gglib/proxy/shutdown", true, &[], Text("text/plain", "shutdown"), (400, "invalid_request")),
        case("a shutdown not confirmed", Method::POST, "/v1/gglib/proxy/shutdown", true, &[], Text(JSON, "{}"), (400, "invalid_request")),
        case("a shutdown with no daemon to stop", Method::POST, "/v1/gglib/proxy/shutdown", true, &[], Text(JSON, r#"{"confirm":"shutdown"}"#), (409, "no_daemon_to_stop")),
        case("a cache clear for a session id that is not one", Method::POST, "/v1/gglib/proxy/cache/clear", true, &[("x-gglib-session-id", "../up")], Empty, (400, "invalid_request")),
        case("a cache clear with the cache on and no slot folder", Method::POST, "/v1/gglib/proxy/cache/clear", true, &[], Empty, (500, "internal_error")),
    ]
}

/// `docs/error-codes.json`, by code: the type and the status it gives.
fn published() -> BTreeMap<String, (Value, Value)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/error-codes.json");
    let file: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    file["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let code = row["code"].as_str().unwrap().to_owned();
            (code, (row["type"].clone(), row["status"].clone()))
        })
        .collect()
}

/// What is wrong with one answer, against its case and the published file.
fn judge(
    case: &Case,
    status: u16,
    content_type: Option<&str>,
    body: &str,
    table: &BTreeMap<String, (Value, Value)>,
) -> Option<String> {
    let mut wrong = Vec::new();
    if status != case.status {
        wrong.push(format!("status {status}, not {}", case.status));
    }
    if content_type != Some(JSON) {
        wrong.push(format!("content type {content_type:?}"));
    }
    let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let error = &parsed["error"];
    let fields: Vec<&String> = parsed
        .as_object()
        .map(|o| o.keys().collect())
        .unwrap_or_default();
    if fields != ["error"] {
        wrong.push("not an object of `error` alone".to_owned());
    }
    if error["message"].as_str().is_none_or(str::is_empty) {
        wrong.push("no message".to_owned());
    }
    if error["code"] != case.code {
        wrong.push(format!("code {}, not {}", error["code"], case.code));
    }
    match table.get(case.code) {
        None => wrong.push(format!("{} is not in docs/error-codes.json", case.code)),
        Some((kind, listed)) => {
            if !kind.is_null() && error["type"] != *kind {
                wrong.push(format!("type {}, not the file's {kind}", error["type"]));
            }
            if !listed.is_null() && *listed != case.status {
                wrong.push(format!("the file gives {} status {listed}", case.code));
            }
        }
    }
    (!wrong.is_empty()).then(|| format!("{}: {} (body {body:.200})", case.what, wrong.join("; ")))
}

#[tokio::test]
async fn every_answer_no_handler_writes_is_the_error_envelope() {
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some(KEY.to_owned()),
        "127.0.0.1",
        vec![],
    );
    // No daemon, and the disk cache on with no slot folder: the two
    // conditions two of the administrative refusals answer.
    let proxy = spawn(ServeConfig {
        access,
        cache_enabled: true,
        slot_dir: None,
        ..defaults().await
    })
    .await;
    let table = published();
    let client = Client::new();

    let mut wrong = Vec::new();
    for case in cases() {
        let url = format!("{}{}", proxy.base, case.path);
        let mut request = client.request(case.method.clone(), url);
        if case.bearer {
            request = request.bearer_auth(KEY);
        }
        for &(name, value) in case.headers {
            request = request.header(
                HeaderName::from_static(name),
                HeaderValue::from_static(value),
            );
        }
        request = match case.body {
            Body::None => request,
            Body::Text(kind, text) => request.header("content-type", kind).body(text),
            Body::Sized(len) => {
                let pad = "x".repeat(len - r#"{"pad":""}"#.len());
                request
                    .header("content-type", JSON)
                    .body(format!(r#"{{"pad":"{pad}"}}"#))
            }
        };
        let response = request.send().await.unwrap();
        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let body = response.text().await.unwrap();
        wrong.extend(judge(&case, status, content_type.as_deref(), &body, &table));
    }
    proxy.cancel.cancel();

    assert!(wrong.is_empty(), "answers not in the envelope: {wrong:#?}");
}

/// A 405 still says which methods the route takes.
#[tokio::test]
async fn a_method_not_allowed_still_names_the_allowed_ones() {
    let proxy = spawn(defaults().await).await;
    let response = Client::new()
        .delete(format!("{}/v1/models", proxy.base))
        .send()
        .await
        .unwrap();
    let allow = response.headers().get("allow").cloned();
    proxy.cancel.cancel();

    assert_eq!(response.status().as_u16(), 405);
    assert!(
        allow.is_some_and(|v| v.to_str().unwrap().contains("GET")),
        "the Allow header names GET"
    );
}
