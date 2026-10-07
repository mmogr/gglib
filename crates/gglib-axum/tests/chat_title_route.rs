//! `POST /api/chat` on the daemon's router: the route takes a title request
//! and nothing else.
//!
//! The harness launches no llama-server, so a well-formed request ends at the
//! port check. What the model is sent, and what is read from its reply, is
//! `handlers/chat_title_tests.rs`.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::authed;

/// What the page sends for a title.
fn title_request() -> Value {
    json!({
        "port": 9000,
        "messages": [{ "role": "user", "content": "What is a tabby?" }],
        "temperature": 0.7,
        "max_tokens": 20,
    })
}

async fn post(app: &Router, body: &Value) -> (StatusCode, String) {
    let request = authed()
        .method(Method::POST)
        .uri("/api/chat")
        .header("Host", "127.0.0.1:9887")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn a_title_request_is_read_and_ends_at_the_port_nothing_runs_on() {
    let app = test_app(CorsConfig::AllowAll).await;

    let (status, body) = post(&app, &title_request()).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body.contains("No running server found on port 9000"),
        "{body}"
    );
}

/// Each of these is a key a general chat completion carries and a title has
/// no use for.
#[tokio::test]
async fn a_request_for_tools_a_stream_or_a_model_is_refused_before_anything_runs() {
    let app = test_app(CorsConfig::AllowAll).await;

    for (key, value) in [
        ("tools", json!([{ "type": "function" }])),
        ("tool_choice", json!("auto")),
        ("stream", json!(true)),
        ("model", json!("qwen")),
        ("reasoningEffort", json!("high")),
    ] {
        let mut request = title_request();
        request[key] = value;

        let (status, body) = post(&app, &request).await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{key}: {body}");
        assert!(body.contains(key), "{key}: {body}");
    }
}

/// Dropped in silence, a misspelt cap would leave the title uncapped.
#[tokio::test]
async fn a_token_cap_under_another_spelling_is_refused_not_dropped() {
    let app = test_app(CorsConfig::AllowAll).await;
    let mut request = title_request();
    let cap = request
        .as_object_mut()
        .unwrap()
        .remove("max_tokens")
        .unwrap();
    request["maxTokens"] = cap;

    let (status, body) = post(&app, &request).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert!(body.contains("maxTokens"), "{body}");
}
