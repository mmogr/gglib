//! A device's turn on a hub chat: read against the hub's record, refused
//! before anything is written when it cannot run, and saved as the chat
//! page's runs are.

use std::time::Duration;

use gglib_core::domain::chat::{ConversationSettings, MessageRole, NewConversation, NewMessage};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::ports::{RunScope, RunsPort as _};
use serde_json::json;

use super::{plan, start};
use crate::error::HttpError;
use crate::handlers::agent::compose::take_permit;
use crate::handlers::agent::launch::launch;
use crate::handlers::agent::run_fixture::{
    End, finished_reply, meta, paced, prepared, saved, saving, settled, state,
};
use crate::state::AppState;

pub(super) fn device(name: &str) -> RunScope {
    RunScope::Device(name.to_owned())
}

pub(super) fn turn(conversation_id: i64, content: &str) -> HubTurn {
    HubTurn {
        conversation_id,
        content: content.to_owned(),
        images: Vec::new(),
        thinking: None,
        answer_saved: false,
    }
}

/// A chat with a system prompt, one question and its reply, made by
/// `qwen3-8b`.
pub(super) async fn chat(state: &AppState, settings: Option<ConversationSettings>) -> i64 {
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
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
                images: Vec::new(),
            })
            .await
            .unwrap();
    }
    id
}

/// The refusal's status and code.
pub(super) fn refused<T>(result: Result<T, HttpError>) -> (u16, &'static str) {
    match result {
        Err(HttpError::Coded { status, code, .. }) => (status.as_u16(), code),
        Err(other) => panic!("uncoded: {other}"),
        Ok(_) => panic!("not refused"),
    }
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
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    let refusal = refused(start(&state, "phone", "d1", turn(id, "hi")).await);
    assert_eq!(refusal, (422, "no_model"));
    assert!(saved(&state, id).await.is_empty());
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
    let created = super::begin(
        &state,
        "phone",
        "d1",
        plan.transcript,
        p,
        take_permit(&state).unwrap(),
    )
    .await
    .unwrap();
    assert!(created.created);
    assert_eq!(created.info.device.as_deref(), Some("phone"));
    assert_eq!(created.info.conversation_id, Some(id));
    assert!(
        state
            .runs
            .existing(&device("phone"), "d1")
            .unwrap()
            .is_some()
    );
    settled(&state).await;

    let rows = saved(&state, id).await;
    let said: Vec<(MessageRole, &str)> =
        rows.iter().map(|r| (r.role, r.content.as_str())).collect();
    assert_eq!(said[2], (MessageRole::User, "second"));
    let device_of = |i: usize| meta(&rows[i], "device");
    assert_eq!(device_of(2), json!("phone"));
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
