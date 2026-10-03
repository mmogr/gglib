//! `POST /api/agent/chat` with `far`: a model of the paired machine, named
//! by that machine and its id there.
//!
//! The unit tests beside `remote_upstream` pin the lookup against a fake far
//! proxy; this pins that the route reaches the far branch at all, and that
//! its refusals are told apart by what is wrong.
//!
//! Neither case needs a tunnel: a ref to this machine is refused before the
//! connection is read, and a ref to a paired machine gets far enough to be
//! refused for the honest reason, which is that nothing is connected.
//!
//! And a conversation the page makes for a far model keeps that model, by
//! its machine, from the moment it is made.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::test_app;
use common::origin::authed;
use gglib_core::CorsConfig;

async fn body_json(response: axum::response::Response) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn chat(body: &str) -> Request<Body> {
    request("POST", "/api/agent/chat".to_owned(), body)
}

/// A model of this machine is driven by its server's port; naming it as a
/// far model is a malformed request, whatever the tunnel is doing.
#[tokio::test]
async fn a_far_ref_to_this_machine_is_a_bad_request() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(chat(
            r#"{"port":0,"messages":[],"far":{"machine":{"kind":"local"},"id":3}}"#,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = body_json(response).await;
    let message = body.get("error").and_then(|v| v.as_str()).unwrap_or("");
    assert!(message.contains("this machine"), "{body}");
}

/// The positive control. A paired ref must get *past* that check — the
/// refusal it then meets is this machine having no tunnel, which is a
/// different sentence and a different status. Without this, the test above
/// would pass just as well against a route that refused everything.
#[tokio::test]
async fn a_far_ref_with_nothing_connected_is_refused_for_the_honest_reason() {
    let app = test_app(CorsConfig::AllowAll).await;

    let response = app
        .oneshot(chat(
            r#"{"port":0,"messages":[],"far":{"machine":{"kind":"paired","fingerprint":"0a1b2c3d4e5f"},"id":3}}"#,
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

fn request(method: &str, uri: String, body: &str) -> Request<Body> {
    authed()
        .method(method)
        .uri(uri)
        .header("Host", "127.0.0.1:9887")
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

/// Make a conversation with `body`, and read back what was stored.
async fn made(body: &str) -> serde_json::Value {
    let app = test_app(CorsConfig::AllowAll).await;
    let created = app
        .clone()
        .oneshot(request("POST", "/api/conversations".to_owned(), body))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let id = body_json(created).await;
    let read = app
        .oneshot(request("GET", format!("/api/conversations/{id}"), ""))
        .await
        .unwrap();
    body_json(read).await
}

/// A conversation made for the paired machine's model keeps it in its
/// settings, so its next turn from either door is that machine's; it names
/// no model of this machine.
#[tokio::test]
async fn a_conversation_made_for_a_far_model_keeps_it() {
    let far = serde_json::json!({ "machine": { "kind": "paired", "fingerprint": "0a1b2c3d4e5f" }, "id": 3 });
    let body =
        serde_json::json!({ "title": "t", "model_id": null, "system_prompt": null, "model": far });

    let stored = made(&body.to_string()).await;

    assert_eq!(stored["settings"]["model"], far, "{stored}");
    assert_eq!(stored["model_id"], serde_json::Value::Null, "{stored}");
}

/// One made for no model stores no settings.
#[tokio::test]
async fn a_conversation_made_for_no_model_names_none() {
    let stored = made(r#"{"title":"t","model_id":null,"system_prompt":null}"#).await;

    assert_eq!(stored["settings"], serde_json::Value::Null, "{stored}");
}
