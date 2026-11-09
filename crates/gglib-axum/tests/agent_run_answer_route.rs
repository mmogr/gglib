//! This machine's turn that answers the question its chat ends in
//! (`answer_saved`), as an edit, a regenerate or Retry leaves it: refused
//! when it carries a message of its own or there is nothing to answer, and
//! made, saving nothing of its own, when there is.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use gglib_core::CorsConfig;
use gglib_core::contracts::http::daemon::run_path;
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewConversation, NewMessage};
use gglib_core::domain::{Machine, ModelRef};
use serde_json::{Value, json};

use common::harness::test_state_and_app;
use common::origin::call_json;

async fn put(app: &Router, id: &str, body: Value) -> (StatusCode, Value) {
    let uri = format!("{}?kind=agent", run_path(id));
    call_json(app, Method::PUT, &uri, Some(body)).await
}

/// This machine's `qwen`, by its id, catalogued on first use: a chat names
/// its model by id, and a name alone is never looked up.
async fn qwen(state: &gglib_axum::AppState) -> ModelRef {
    let found = state.core.models().get("qwen").await.unwrap();
    let id = if let Some(model) = found {
        model.id
    } else {
        let model = gglib_core::domain::NewModel::new(
            "qwen".to_owned(),
            std::path::PathBuf::from("/models/qwen.gguf"),
            7.0,
            chrono::Utc::now(),
        );
        state.core.models().add(model).await.unwrap().id
    };
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

/// A conversation holding `rows`, in order; its id.
async fn conversation(state: &gglib_axum::AppState, rows: &[(MessageRole, &str)]) -> i64 {
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "t".into(),
            settings: Some(ConversationSettings {
                model: Some(qwen(state).await),
                model_name: Some("qwen".to_owned()),
                ..ConversationSettings::default()
            }),
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

fn answering(conversation: i64) -> Value {
    json!({ "conversation_id": conversation, "content": "", "answer_saved": true })
}

/// An answer to a question that is saved is made, and saves nothing of its
/// own: the question stays the chat's last row. Here its model cannot load,
/// so the run fails with nothing written.
#[tokio::test]
async fn an_answer_to_a_saved_question_is_made_and_saves_no_message() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = conversation(&state, &[(MessageRole::User, "Q")]).await;

    let (status, info) = put(&app, "e1", answering(id)).await;

    assert_eq!(status, StatusCode::CREATED, "{info}");
    assert_eq!(contents(&state, id).await, ["Q"]);
}

#[tokio::test]
async fn an_answer_carries_no_message_of_its_own() {
    let (state, app) = test_state_and_app(CorsConfig::AllowAll).await;
    let id = conversation(&state, &[(MessageRole::User, "Q")]).await;
    let mut speaking = answering(id);
    speaking["content"] = json!("Q2");
    let mut showing = answering(id);
    showing["images"] = json!([gglib_core::domain::AttachmentId::of(b"x").to_string()]);

    for body in [speaking, showing] {
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

    let (status, error) = put(&app, "e1", answering(id)).await;

    assert_eq!(status, StatusCode::CONFLICT, "{error}");
    assert_eq!(error["type"], "nothing_to_answer");
    assert_eq!(contents(&state, id).await, ["Q", "A"]);
}
