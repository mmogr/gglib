//! A message or a chat is not deleted while a reply to the chat is still
//! being written: the reply would be saved after rows that are gone, or to
//! a chat that is.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use gglib_app_services::{Reservation, RunSpec};
use gglib_core::CorsConfig;
use gglib_core::domain::chat::{MessageRole, NewConversation, NewMessage};
use gglib_core::domain::runs::RunKind;
use gglib_core::ports::RunScope;
use serde_json::Value;

use common::harness::test_state_and_app;
use common::origin::call_json;

/// A conversation holding a question and its answer; its id and the
/// question's.
async fn conversation(state: &gglib_axum::AppState) -> (i64, i64) {
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "t".into(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let row = |role, content: &str| NewMessage {
        conversation_id: id,
        role,
        content: content.to_owned(),
        metadata: None,
        images: Vec::new(),
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

async fn delete(app: &Router, uri: &str) -> (StatusCode, Value) {
    call_json(app, Method::DELETE, uri, None).await
}

fn spec(conversation: i64) -> RunSpec {
    RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: Some(conversation),
    }
}

#[tokio::test]
async fn a_chat_being_written_keeps_its_messages_and_itself() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (id, question) = conversation(&state).await;
    let Ok(Reservation::New(writing)) = state.runs.reserve(RunScope::Local, "e1", spec(id)) else {
        panic!("reserved");
    };

    for uri in [
        format!("/api/messages/{question}"),
        format!("/api/conversations/{id}"),
    ] {
        let (status, error) = delete(&app, &uri).await;
        assert_eq!(status, StatusCode::CONFLICT, "{uri}: {error}");
        assert_eq!(error["type"], "conflict");
        assert!(
            error["error"].as_str().unwrap().contains("run e1"),
            "{error}"
        );
    }
    let rows = state.core.chat_history().get_messages(id).await.unwrap();
    assert_eq!(rows.len(), 2);

    drop(writing);
    let (status, error) = delete(&app, &format!("/api/messages/{question}")).await;
    assert_eq!(status, StatusCode::OK, "{error}");
    let (status, error) = delete(&app, &format!("/api/conversations/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{error}");
}

#[tokio::test]
async fn a_message_no_chat_has_is_not_found() {
    let (_state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let (status, error) = delete(&app, "/api/messages/404").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{error}");
}
