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
//! its machine, from the moment it is made, as one made for a model of this
//! machine keeps that one, by its id too.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use common::harness::{test_app, test_state_and_app};
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
    made_in(test_app(CorsConfig::AllowAll).await, body).await
}

/// [`made`], in `app`.
async fn made_in(app: axum::Router, body: &str) -> serde_json::Value {
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

/// A daemon with one model in its catalogue: its router, and the model's id.
async fn with_a_model() -> (axum::Router, i64) {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let model = gglib_core::domain::NewModel::new(
        "qwen3".to_owned(),
        std::path::PathBuf::from("/models/qwen3.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    let id = state.core.models().add(model).await.unwrap().id;
    (app, id)
}

/// One made for a model of this machine names it in its settings and by its
/// `model_id`, and the two agree: the settings' model decides the id.
#[tokio::test]
async fn a_conversation_made_for_a_local_model_names_it_by_its_id_too() {
    let (app, id) = with_a_model().await;
    let here = serde_json::json!({ "machine": { "kind": "local" }, "id": id });
    let body =
        serde_json::json!({ "title": "t", "model_id": null, "system_prompt": "p", "model": here });

    let stored = made_in(app, &body.to_string()).await;

    assert_eq!(stored["settings"], serde_json::json!({ "model": here }));
    assert_eq!(stored["model_id"], id, "{stored}");
    assert_eq!(stored["system_prompt"], "p", "{stored}");
}

/// One made with a `model_id` and no model keeps the id, and stores no
/// settings; a far model beside the id leaves the id out, since it is not
/// that machine's.
#[tokio::test]
async fn a_model_id_is_kept_unless_the_conversation_is_made_for_a_far_model() {
    let (app, id) = with_a_model().await;
    let far = serde_json::json!({ "machine": { "kind": "paired", "fingerprint": "0a1b2c3d4e5f" }, "id": id });
    let alone = serde_json::json!({ "title": "t", "model_id": id, "system_prompt": null });
    let beside =
        serde_json::json!({ "title": "t", "model_id": id, "system_prompt": null, "model": far });

    let kept = made_in(app.clone(), &alone.to_string()).await;
    let left_out = made_in(app, &beside.to_string()).await;

    assert_eq!(kept["model_id"], id, "{kept}");
    assert_eq!(kept["settings"], serde_json::Value::Null, "{kept}");
    assert_eq!(left_out["model_id"], serde_json::Value::Null, "{left_out}");
    assert_eq!(left_out["settings"]["model"], far, "{left_out}");
}
