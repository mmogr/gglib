//! The model a device's turn runs on when its chat names none: the one
//! running on the hub, before the hub's default.

use gglib_app_services::types::ServerInfo;
use gglib_core::domain::chat::{Conversation, ConversationSettings, Message, MessageRole};

use super::choose;
use crate::error::HttpError;
use crate::handlers::agent::run_fixture::state;
use crate::state::AppState;

fn server(model_name: &str, port: u16, started_at: u64) -> ServerInfo {
    ServerInfo {
        model_id: 1,
        model_name: model_name.to_owned(),
        pid: None,
        port,
        started_at,
    }
}

async fn registered(state: &AppState, name: &str) -> i64 {
    let model = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(format!("/models/{name}.gguf")),
        7.0,
        chrono::Utc::now(),
    );
    state.core.models().add(model).await.unwrap().id
}

/// A chat that names no model anywhere.
async fn bare(state: &AppState) -> (Conversation, Vec<Message>) {
    let history = state.core.chat_history();
    let id = history
        .create_conversation("t".to_owned(), None, None)
        .await
        .unwrap();
    (
        history.get_conversation(id).await.unwrap().unwrap(),
        Vec::new(),
    )
}

#[tokio::test]
async fn a_chat_naming_no_model_runs_on_the_one_running() {
    let (_dir, state) = state().await;
    let (conversation, rows) = bare(&state).await;
    let running = [server("running-7b", 9000, 10)];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "running-7b");
}

/// Of several, the one started last; of those started together, the lowest
/// port.
#[tokio::test]
async fn of_several_running_the_one_started_last() {
    let (_dir, state) = state().await;
    let (conversation, rows) = bare(&state).await;
    // Neither first nor last, so taking either end of the list is wrong.
    let running = [
        server("late-high", 9002, 20),
        server("late-low", 9001, 20),
        server("early", 9000, 10),
    ];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "late-low");
}

#[tokio::test]
async fn nothing_running_and_no_default_says_to_start_a_model() {
    let (_dir, state) = state().await;
    let (conversation, rows) = bare(&state).await;
    match choose(&state, &conversation, &rows, &[]).await {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => {
            assert_eq!((status.as_u16(), code), (422, "no_model"));
            assert_eq!(
                message,
                "this chat has no model and nothing is running on the hub: start a model there"
            );
        }
        other => panic!("not refused as no_model: {:?}", other.map(|_| ())),
    }
}

/// The chat's own model, its settings' and its last reply's each come
/// before the one running.
#[tokio::test]
async fn what_the_chat_names_wins_over_what_is_running() {
    let (_dir, state) = state().await;
    let running = [server("running-7b", 9000, 10)];
    let (mut conversation, _) = bare(&state).await;
    let reply = Message {
        id: 1,
        conversation_id: conversation.id,
        role: MessageRole::Assistant,
        content: "answer".to_owned(),
        created_at: String::new(),
        metadata: Some(serde_json::json!({ "modelName": "replied-model" })),
    };
    let rows = [reply];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "replied-model", "its last reply's");

    conversation.settings = Some(ConversationSettings {
        model_name: Some("set-model".to_owned()),
        ..ConversationSettings::default()
    });
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "set-model", "its settings'");

    conversation.model_id = Some(registered(&state, "own-model").await);
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "own-model", "its own");
}

/// The hub's default only when nothing runs.
#[tokio::test]
async fn the_one_running_comes_before_the_hubs_default() {
    let (_dir, state) = state().await;
    let mut settings = state.core.settings().get().await.unwrap();
    settings.default_model_id = Some(registered(&state, "default-model").await);
    state.core.settings().save(&settings).await.unwrap();
    let (conversation, rows) = bare(&state).await;
    let running = [server("running-7b", 9000, 10)];
    let model = choose(&state, &conversation, &rows, &running).await;
    assert_eq!(model.unwrap(), "running-7b");
    let model = choose(&state, &conversation, &rows, &[]).await;
    assert_eq!(model.unwrap(), "default-model");
}
