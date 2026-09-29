//! A run that replaces rows (an edit, a regenerate) changes nothing when it
//! is refused: the rows are deleted only once the run is accepted, in the
//! transaction that saves the user's message.

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::run_path;
use gglib_core::domain::chat::{MessageRole, NewMessage};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::harness::test_state_and_app;

async fn put(app: &Router, id: &str, body: Value) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(Method::PUT)
        .uri(format!("{}?kind=agent", run_path(id)))
        .header("Host", "127.0.0.1:9887")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

/// A conversation holding a question and its answer; its id and the
/// question's.
async fn conversation(state: &gglib_axum::AppState) -> (i64, i64) {
    let history = state.core.chat_history();
    let id = history
        .create_conversation("t".into(), None, None)
        .await
        .unwrap();
    let row = |role, content: &str| NewMessage {
        conversation_id: id,
        role,
        content: content.to_owned(),
        metadata: None,
    };
    let question = history
        .save_message(row(MessageRole::User, "Q"))
        .await
        .unwrap();
    history
        .save_message(row(MessageRole::Assistant, "A"))
        .await
        .unwrap();
    (id, question)
}

async fn contents(state: &gglib_axum::AppState, id: i64) -> Vec<String> {
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    rows.into_iter().map(|r| r.content).collect()
}

fn replacing(conversation: i64, from: i64) -> Value {
    json!({
        "port": 9000,
        "messages": [{ "role": "user", "content": "Q2" }],
        "conversation_id": conversation,
        "replace_from": from,
    })
}

#[tokio::test]
async fn a_busy_refusal_replaces_nothing() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, question) = conversation(&state).await;
    let slots = state.agent_semaphore.available_permits();
    let _held = std::sync::Arc::clone(&state.agent_semaphore)
        .try_acquire_many_owned(u32::try_from(slots).unwrap())
        .unwrap();

    let (status, error) = put(&app, "e1", replacing(id, question)).await;

    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{error}");
    assert_eq!(contents(&state, id).await, ["Q", "A"]);
}

#[tokio::test]
async fn a_refusal_of_the_request_itself_replaces_nothing() {
    // No llama-server in the harness: the port is refused, as the chat
    // route refuses it, after the slot is taken.
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, question) = conversation(&state).await;

    let (status, error) = put(&app, "e1", replacing(id, question)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(contents(&state, id).await, ["Q", "A"]);
}

#[tokio::test]
async fn replacing_needs_a_conversation_and_a_users_message_last() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, question) = conversation(&state).await;
    let mut no_conversation = replacing(id, question);
    no_conversation["conversation_id"] = Value::Null;
    let mut not_the_user = replacing(id, question);
    not_the_user["messages"] = json!([{ "role": "system", "content": "S" }]);

    for body in [no_conversation, not_the_user] {
        let (status, error) = put(&app, "e1", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["type"], "invalid_request");
    }
    assert_eq!(contents(&state, id).await, ["Q", "A"]);
}
