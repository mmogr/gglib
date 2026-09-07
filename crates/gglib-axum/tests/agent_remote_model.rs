//! `POST /api/agent/chat` with `"remote": true` and no model named.
//!
//! The unit tests beside `remote_upstream::remote_model` pin what the refusal
//! says; this pins that the route reaches it at all. Without that, the guard
//! is a mechanism with no evidence it runs — and the defect it exists for was
//! precisely a request travelling all the way to another machine before
//! anything looked at it.
//!
//! Neither case needs a tunnel: an unnamed model is refused before the
//! connection is read, and a named one gets far enough to be refused for the
//! honest reason instead.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use gglib_core::CorsConfig;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn chat(body: &'static str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/agent/chat")
        .header("Host", "127.0.0.1:9887")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[tokio::test]
async fn a_remote_turn_with_no_model_is_refused_before_it_leaves_this_machine() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(chat(r#"{"port":9000,"messages":[],"remote":true}"#))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "an unnamed model used to travel and come back `404 Model '' not found`"
    );
    let body = body_json(response).await;
    let message = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        message.contains("no model named"),
        "the refusal has to name the real problem: {body}"
    );
}

/// The positive control. A named model must get *past* the model guard — the
/// refusal it then meets is this machine having no tunnel, which is a
/// different sentence and a different status. Without this, the test above
/// would pass just as well against a route that refused everything.
#[tokio::test]
async fn a_named_model_gets_past_the_guard_and_is_refused_for_the_honest_reason() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(chat(
            r#"{"port":9000,"messages":[],"remote":true,"model":"qwen3"}"#,
        ))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "nothing is connected here, and that is what should be said"
    );
    let body = body_json(response).await;
    let message = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(
        message.contains("not connected"),
        "expected the not-connected refusal, got: {body}"
    );
}
