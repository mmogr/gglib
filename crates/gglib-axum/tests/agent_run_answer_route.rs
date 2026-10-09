//! A run that answers the question its conversation ends in changes
//! nothing when it is refused, and is refused when it names no
//! conversation, sends messages of its own, or has nothing to answer.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::run_path;
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use serde_json::{Value, json};

use common::harness::test_state_and_app;
use common::origin::call_json;

async fn put(app: &Router, id: &str, body: Value) -> (StatusCode, Value) {
    let uri = format!("{}?kind=agent", run_path(id));
    call_json(app, Method::PUT, &uri, Some(body)).await
}

/// A conversation holding `rows`, in order; its id.
async fn conversation(state: &gglib_axum::AppState, rows: &[(MessageRole, &str)]) -> i64 {
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "t".into(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    for (role, content) in rows {
        let row = NewMessage {
            conversation_id: id,
            role: *role,
            content: (*content).to_owned(),
            metadata: None,
            images: Vec::new(),
        };
        history.save_message(row).await.unwrap();
    }
    id
}

async fn contents(state: &gglib_axum::AppState, id: i64) -> Vec<String> {
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    rows.into_iter().map(|r| r.content).collect()
}

fn answering(conversation: Option<i64>) -> Value {
    json!({
        "port": 9000,
        "messages": [],
        "conversation_id": conversation,
        "answer_saved": true,
    })
}

#[tokio::test]
async fn a_refusal_of_the_request_itself_saves_nothing() {
    // No llama-server in the harness: the port is refused, as the chat
    // route refuses it, after the history is read.
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = conversation(&state, &[(MessageRole::User, "Q")]).await;

    let (status, error) = put(&app, "e1", answering(Some(id))).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
    assert_eq!(contents(&state, id).await, ["Q"]);
}

#[tokio::test]
async fn an_answer_names_its_conversation_and_sends_no_messages() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = conversation(&state, &[(MessageRole::User, "Q")]).await;
    let mut speaking = answering(Some(id));
    speaking["messages"] = json!([{ "role": "user", "content": "Q2" }]);

    for body in [answering(None), speaking] {
        let (status, error) = put(&app, "e1", body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["type"], "invalid_request");
    }
    assert_eq!(contents(&state, id).await, ["Q"]);
}

#[tokio::test]
async fn a_chat_that_ends_in_a_reply_has_nothing_to_answer() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = conversation(
        &state,
        &[(MessageRole::User, "Q"), (MessageRole::Assistant, "A")],
    )
    .await;

    let (status, error) = put(&app, "e1", answering(Some(id))).await;

    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    assert_eq!(error["type"], "nothing_to_answer");
    assert_eq!(contents(&state, id).await, ["Q", "A"]);
}
