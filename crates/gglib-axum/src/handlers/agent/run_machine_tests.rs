//! A conversation's machine is fixed once it has a saved message: a run on
//! another machine than the one it ran on ends `failed`, `conflict`, before
//! anything is written, in both directions. Before that, the model it names
//! was only chosen, and a run on any machine is not refused.

use gglib_core::domain::chat::{
    Conversation, ConversationSettings, MessageRole, NewConversation, NewMessage,
};
use gglib_core::domain::runs::{RunError, RunStatus};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::RunsPort as _;

use super::compose::{Prepared, take_permit};
use super::run_fixture::launch;
use super::run_fixture::{End, LOCAL, finished_reply, prepared, saved, saving, settled, state};
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

/// Run `p` on `conversation`; how its run ended: with no error, or the
/// one it failed with.
async fn launched(state: &AppState, conversation: i64, p: Prepared) -> Result<(), RunError> {
    let permit = || take_permit(state).expect("a free slot");
    launch(state, "m1", LOCAL, saving(conversation), p, permit()).expect("reserved");
    settled(state).await;
    let ended = state.runs.get(&LOCAL, "m1").unwrap();
    match ended.status {
        RunStatus::Completed => Ok(()),
        _ => Err(ended.error.expect("a failed run says why")),
    }
}

async fn read(state: &AppState, id: i64) -> Conversation {
    let history = state.core.chat_history();
    history.get_conversation(id).await.unwrap().unwrap()
}

/// Save a message to conversation `id`, so it has run.
async fn said(state: &AppState, id: i64) {
    let message = NewMessage {
        conversation_id: id,
        role: MessageRole::User,
        content: "Q".to_owned(),
        metadata: None,
        images: Vec::new(),
    };
    state
        .core
        .chat_history()
        .save_message(message)
        .await
        .unwrap();
}

/// Conversation `id`, once it has a saved message, ends the run `failed`,
/// `conflict`, in words that name `words`, with no message saved and the
/// stored model as it was.
async fn refused(state: &AppState, id: i64, p: Prepared, words: &str) {
    said(state, id).await;
    let before = read(state, id).await.settings.and_then(|s| s.model);
    let error = launched(state, id, p).await.expect_err("refused");
    assert_eq!(error.code, "conflict");
    assert!(error.message.contains(words), "{}", error.message);
    assert_eq!(saved(state, id).await.len(), 1, "nothing was saved");
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

/// A model chosen for a chat with no saved message fixes nothing: a run on
/// another machine's model is not refused.
#[tokio::test]
async fn a_chat_with_no_message_takes_a_run_on_another_machine() {
    let (_dir, state) = state().await;
    let id = made_for(&state, there("0a1b2c3d4e5f")).await;
    launched(&state, id, here()).await.expect("a local run");
    assert!(!saved(&state, id).await.is_empty());
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
