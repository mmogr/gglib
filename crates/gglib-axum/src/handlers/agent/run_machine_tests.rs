//! A conversation's machine is fixed: a run on another machine than the one
//! it ran on is refused before anything is written, in both directions.

use axum::http::StatusCode;
use gglib_core::domain::chat::{Conversation, ConversationSettings, NewConversation};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::RunsPort as _;

use super::compose::{Prepared, take_permit};
use super::launch::launch;
use super::run_fixture::{End, LOCAL, finished_reply, prepared, saved, saving, settled, state};
use crate::error::HttpError;
use crate::state::AppState;

fn paired(fingerprint: &str) -> Machine {
    Machine::Paired {
        fingerprint: fingerprint.to_owned(),
    }
}

/// Model 5 of the machine `fingerprint` names.
fn there(fingerprint: &str) -> ModelRef {
    ModelRef {
        machine: paired(fingerprint),
        id: 5,
    }
}

/// A model in this machine's registry, by its id.
async fn registered(state: &AppState) -> i64 {
    let model = gglib_core::domain::NewModel::new(
        "qwen3".to_owned(),
        std::path::PathBuf::from("/models/qwen3.gguf"),
        7.0,
        chrono::Utc::now(),
    );
    state.core.models().add(model).await.unwrap().id
}

/// A conversation that stores `model`, and so the `model_id` it implies.
async fn made_for(state: &AppState, model: ModelRef) -> i64 {
    let settings = ConversationSettings {
        model: Some(model),
        ..Default::default()
    };
    state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            settings: Some(settings),
            ..NewConversation::default()
        })
        .await
        .unwrap()
}

/// A run on this machine's model 5.
fn here() -> Prepared {
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.local_model = Some((9000, 5));
    p
}

/// A run on model 5 of the machine `fingerprint` names.
fn on(fingerprint: &str) -> Prepared {
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.far_model = Some(there(fingerprint));
    p
}

async fn launched(state: &AppState, conversation: i64, p: Prepared) -> Result<(), HttpError> {
    let created = launch(
        state,
        "m1",
        LOCAL,
        saving(conversation),
        p,
        take_permit(state).expect("a free slot"),
    )
    .await;
    settled(state).await;
    created.map(|_| ())
}

async fn read(state: &AppState, id: i64) -> Conversation {
    let history = state.core.chat_history();
    history.get_conversation(id).await.unwrap().unwrap()
}

/// Refused with a `409` that names `words`, with no message saved, no run
/// left behind and the stored model as it was.
async fn refused(state: &AppState, id: i64, p: Prepared, words: &str) {
    let before = read(state, id).await.settings.and_then(|s| s.model);
    let error = launched(state, id, p).await.expect_err("refused");
    let HttpError::Coded {
        status, message, ..
    } = &error
    else {
        panic!("a coded refusal, not {error:?}");
    };
    assert_eq!(*status, StatusCode::CONFLICT);
    assert!(message.contains(words), "{message}");
    assert!(saved(state, id).await.is_empty(), "nothing was saved");
    assert!(state.runs.get(&LOCAL, "m1").is_err(), "no run is left");
    let after = read(state, id).await.settings.and_then(|s| s.model);
    assert_eq!(after, before, "the chat keeps its machine");
}

#[tokio::test]
async fn a_far_run_on_a_chat_of_this_machine_is_refused() {
    let (_dir, state) = state().await;
    let model = registered(&state).await;
    let local = ModelRef {
        machine: Machine::Local,
        id: model,
    };
    let id = made_for(&state, local).await;
    refused(&state, id, on("0a1b2c3d4e5f"), "continues here").await;
}

/// A chat with only a `model_id` ran here too: its rows predate the stored
/// model.
#[tokio::test]
async fn a_far_run_on_a_chat_with_a_model_id_is_refused() {
    let (_dir, state) = state().await;
    let model = registered(&state).await;
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: Some(model),
            ..NewConversation::default()
        })
        .await
        .unwrap();
    refused(&state, id, on("0a1b2c3d4e5f"), "continues here").await;
}

#[tokio::test]
async fn a_local_run_on_a_chat_of_the_paired_machine_is_refused() {
    let (_dir, state) = state().await;
    let id = made_for(&state, there("0a1b2c3d4e5f")).await;
    refused(&state, id, here(), "continues there").await;
}

/// The same id on a machine paired since is another model.
#[tokio::test]
async fn a_run_on_another_paired_machine_is_refused() {
    let (_dir, state) = state().await;
    let id = made_for(&state, there("0a1b2c3d4e5f")).await;
    refused(&state, id, on("ffeeddccbbaa"), "no longer paired with").await;
}

/// A chat that stores no model takes the first run's machine, and keeps it.
#[tokio::test]
async fn a_chat_with_no_model_takes_the_runs_machine() {
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
    launched(&state, id, on("0a1b2c3d4e5f")).await.expect("run");
    let stored = read(&state, id).await.settings.and_then(|s| s.model);
    assert_eq!(stored.map(|m| m.machine), Some(paired("0a1b2c3d4e5f")));
}

#[tokio::test]
async fn a_run_on_the_chats_own_machine_is_not_refused() {
    let (_dir, state) = state().await;
    let far = made_for(&state, there("0a1b2c3d4e5f")).await;
    launched(&state, far, on("0a1b2c3d4e5f"))
        .await
        .expect("a far run");
    assert!(!saved(&state, far).await.is_empty());
}
