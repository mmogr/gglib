//! A device's turn on a hub chat: read against the hub's record, refused
//! before anything is written when it cannot run, and saved as the chat
//! page's runs are.

use std::time::Duration;

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewConversation, NewMessage};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::ports::{RunScope, RunsPort as _};
use serde_json::json;

use super::{plan, start};
use crate::error::HttpError;
use crate::handlers::agent::compose::take_permit;
use crate::handlers::agent::hub_model::model_for;
use crate::handlers::agent::launch::launch;
use crate::handlers::agent::run_fixture::{
    End, finished_reply, paced, prepared, saved, saving, settled, state,
};
use crate::state::AppState;

fn device(name: &str) -> RunScope {
    RunScope::Device(name.to_owned())
}

fn turn(conversation_id: i64, content: &str) -> HubTurn {
    HubTurn {
        conversation_id,
        content: content.to_owned(),
    }
}

/// A chat with a system prompt, one question and its reply, made by
/// `qwen3-8b`.
async fn chat(state: &AppState, settings: Option<ConversationSettings>) -> i64 {
    let history = state.core.chat_history();
    let id = history
        .create_conversation_with_settings(NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: Some("  Be brief.  ".to_owned()),
            settings,
        })
        .await
        .unwrap();
    for (role, content, metadata) in [
        (MessageRole::User, "first", None),
        (
            MessageRole::Assistant,
            "answer",
            Some(json!({ "modelName": "qwen3-8b" })),
        ),
    ] {
        history
            .save_message(NewMessage {
                conversation_id: id,
                role,
                content: content.to_owned(),
                metadata,
            })
            .await
            .unwrap();
    }
    id
}

/// The refusal's status and code.
fn refused<T>(result: Result<T, HttpError>) -> (u16, &'static str) {
    match result {
        Err(HttpError::Coded { status, code, .. }) => (status.as_u16(), code),
        Err(other) => panic!("uncoded: {other}"),
        Ok(_) => panic!("not refused"),
    }
}

#[tokio::test]
async fn the_history_is_the_prompt_the_rows_and_the_new_message() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let plan = plan(&state, turn(id, "second")).await.unwrap();
    let user = |c: &str| AgentMessage::User {
        content: c.to_owned(),
    };
    let wire: Vec<String> = plan
        .chat
        .messages
        .iter()
        .map(|m| serde_json::to_string(m).unwrap())
        .collect();
    let want: Vec<String> = [
        AgentMessage::System {
            content: "Be brief.".to_owned(),
        },
        user("first"),
        AgentMessage::Assistant {
            content: gglib_core::domain::agent::AssistantContent {
                text: Some("answer".to_owned()),
                tool_calls: Vec::new(),
            },
        },
        user("second"),
    ]
    .iter()
    .map(|m| serde_json::to_string(m).unwrap())
    .collect();
    assert_eq!(wire, want);
    assert_eq!(plan.model, "qwen3-8b", "the model of the last reply");
    assert!(plan.chat.config.is_none() && plan.chat.tool_filter.is_none());
}

#[tokio::test]
async fn the_conversations_settings_set_the_limits_and_the_tools() {
    let (_dir, state) = state().await;
    let settings = ConversationSettings {
        max_iterations: Some(4),
        tools: vec!["fs:read_file".to_owned()],
        ..ConversationSettings::default()
    };
    let id = chat(&state, Some(settings)).await;
    let plan = plan(&state, turn(id, "second")).await.unwrap();
    assert_eq!(plan.chat.config.and_then(|c| c.max_iterations), Some(4));
    assert_eq!(plan.chat.tool_filter, Some(vec!["fs:read_file".to_owned()]));

    let off = ConversationSettings {
        no_tools: Some(true),
        ..ConversationSettings::default()
    };
    let id = chat(&state, Some(off)).await;
    let plan = super::plan(&state, turn(id, "second")).await.unwrap();
    assert_eq!(plan.chat.tool_filter, Some(Vec::new()));
}

#[tokio::test]
async fn an_empty_message_is_400_and_writes_nothing() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    for content in ["", "  \n "] {
        let refusal = refused(start(&state, "phone", "d1", turn(id, content)).await);
        assert_eq!(refusal, (400, "invalid_request"));
    }
    assert_eq!(saved(&state, id).await.len(), 2);
    assert!(state.runs.list(&device("phone")).runs.is_empty());
}

#[tokio::test]
async fn a_missing_chat_is_404() {
    let (_dir, state) = state().await;
    let refusal = refused(start(&state, "phone", "d1", turn(4242, "hi")).await);
    assert_eq!(refusal, (404, "conversation_not_found"));
}

#[tokio::test]
async fn a_chat_that_names_no_model_is_refused_before_anything_runs() {
    let (_dir, state) = state().await;
    let id = state
        .core
        .chat_history()
        .create_conversation("t".to_owned(), None, None)
        .await
        .unwrap();
    let refusal = refused(start(&state, "phone", "d1", turn(id, "hi")).await);
    assert_eq!(refusal, (409, "no_model"));
    assert!(saved(&state, id).await.is_empty());
}

#[tokio::test]
async fn the_chats_own_model_comes_before_the_one_it_last_used() {
    let (_dir, state) = state().await;
    let model = gglib_core::domain::NewModel::new(
        "catalogued".to_owned(),
        std::path::PathBuf::from("/models/c.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    let model_id = state.core.models().add(model).await.unwrap().id;
    let id = chat(&state, None).await;
    let history = state.core.chat_history();
    let mut conversation = history.get_conversation(id).await.unwrap().unwrap();
    let rows = history.get_messages(id).await.unwrap();
    assert_eq!(
        model_for(&state, &conversation, &rows).await.unwrap(),
        "qwen3-8b"
    );
    conversation.model_id = Some(model_id);
    assert_eq!(
        model_for(&state, &conversation, &rows).await.unwrap(),
        "catalogued"
    );
}

/// The device's run saves the message it sent and, once the loop ends, the
/// reply, to the hub's chat.
#[tokio::test]
async fn a_device_turn_saves_its_message_and_the_reply() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let plan = plan(&state, turn(id, "second")).await.unwrap();
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.messages = plan.chat.messages;
    let created = launch(
        &state,
        "d1",
        device("phone"),
        saving(id),
        p,
        take_permit(&state).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(created.info.device.as_deref(), Some("phone"));
    assert_eq!(created.info.conversation_id, Some(id));
    settled(&state).await;

    let rows = saved(&state, id).await;
    let said: Vec<(MessageRole, &str)> =
        rows.iter().map(|r| (r.role, r.content.as_str())).collect();
    assert_eq!(said[2], (MessageRole::User, "second"));
    assert_eq!(
        said.last().copied(),
        Some((MessageRole::Assistant, "ANSWER-SECRET"))
    );
}

/// One live reply per chat: a second device's turn is refused while the
/// first's reply is not saved, before a model is looked for.
#[tokio::test]
async fn a_second_turn_on_a_chat_with_a_live_reply_is_409() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let (p, _) = paced(finished_reply(), End::Hang, Duration::from_millis(1));
    launch(
        &state,
        "d1",
        device("phone"),
        saving(id),
        p,
        take_permit(&state).unwrap(),
    )
    .await
    .unwrap();

    let refusal = refused(start(&state, "laptop", "d2", turn(id, "again")).await);
    assert_eq!(refusal, (409, "conflict"));
    let rows = saved(&state, id).await;
    assert!(rows.iter().all(|r| r.content != "again"), "nothing written");
    state.runs.shutdown();
}

/// The same id from the same device is its run, answered as it stands.
#[tokio::test]
async fn a_repeated_id_answers_with_the_devices_run() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let (p, _) = paced(finished_reply(), End::Hang, Duration::from_millis(1));
    launch(
        &state,
        "d1",
        device("phone"),
        saving(id),
        p,
        take_permit(&state).unwrap(),
    )
    .await
    .unwrap();
    let again = start(&state, "phone", "d1", turn(id, "again"))
        .await
        .unwrap();
    assert!(!again.created);
    let taken = refused(start(&state, "laptop", "d1", turn(id, "again")).await);
    assert_eq!(taken, (409, "conflict"));
    state.runs.shutdown();
}

/// The proxy is handed the refusal as the daemon's door would answer it.
#[tokio::test]
async fn the_starter_hands_the_proxy_the_coded_refusal() {
    use gglib_core::ports::AgentRunStarter as _;
    let (_dir, state) = state().await;
    let starter = super::HubTurns(std::sync::Arc::downgrade(&state));
    let refusal = starter.start("phone", "d1", turn(4242, "hi")).await.err();
    let refusal = refusal.expect("refused");
    assert_eq!(
        (refusal.status, refusal.code.as_str()),
        (404, "conversation_not_found")
    );
    assert_eq!(refusal.message, "no conversation has id 4242");
}
