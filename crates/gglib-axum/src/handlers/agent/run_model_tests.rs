//! A run names the model it uses on its conversation, from either door, so
//! the chat's next turn runs on it.

use gglib_core::domain::chat::{Conversation, NewConversation};
use gglib_core::ports::RunScope;

use super::compose::{Prepared, take_permit};
use super::launch::launch;
use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, prepared, saving, settled, state,
};
use crate::state::AppState;

async fn registered(state: &AppState, name: &str) -> i64 {
    let model = gglib_core::domain::NewModel::new(
        name.to_owned(),
        std::path::PathBuf::from(format!("/models/{name}.gguf")),
        7.0,
        chrono::Utc::now(),
    );
    state.core.models().add(model).await.unwrap().id
}

/// A run on the local model `model_id`, served as `name`.
fn on(model_id: i64, name: &str) -> Prepared {
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.local_model = Some((9000, model_id));
    name.clone_into(&mut p.made_by.model);
    p
}

/// Run `p` to its end as run `id` in `scope`, saved to `conversation`.
async fn run(state: &AppState, scope: RunScope, id: &str, conversation: i64, p: Prepared) {
    let created = launch(
        state,
        id,
        scope,
        saving(conversation),
        p,
        take_permit(state).expect("a free slot"),
    )
    .await
    .expect("started");
    assert!(created.created);
    settled(state).await;
}

async fn read(state: &AppState, id: i64) -> Conversation {
    let history = state.core.chat_history();
    history.get_conversation(id).await.unwrap().unwrap()
}

fn named(conversation: &Conversation) -> Option<&str> {
    conversation.settings.as_ref()?.model_name.as_deref()
}

#[tokio::test]
async fn a_page_run_names_its_model_on_a_chat_that_had_none() {
    let (_dir, state) = state().await;
    let model = registered(&state, "qwen3-8b").await;
    let id = conversation(&state).await;
    run(&state, LOCAL, "m1", id, on(model, "qwen3-8b")).await;
    let after = read(&state, id).await;
    assert_eq!(
        (after.model_id, named(&after)),
        (Some(model), Some("qwen3-8b"))
    );
}

#[tokio::test]
async fn a_device_turn_names_its_model_on_a_chat_that_had_none() {
    let (_dir, state) = state().await;
    let model = registered(&state, "qwen3-8b").await;
    let id = conversation(&state).await;
    let phone = RunScope::Device("phone".to_owned());
    run(&state, phone, "m1", id, on(model, "qwen3-8b")).await;
    let after = read(&state, id).await;
    assert_eq!(
        (after.model_id, named(&after)),
        (Some(model), Some("qwen3-8b"))
    );
}

/// What the run used replaces what the chat named; its other settings stay.
#[tokio::test]
async fn a_run_on_another_model_replaces_the_chats_and_keeps_its_settings() {
    let (_dir, state) = state().await;
    let old = registered(&state, "old-model").await;
    let used = registered(&state, "new-model").await;
    let settings = gglib_core::domain::chat::ConversationSettings {
        model_name: Some("old-model".to_owned()),
        max_iterations: Some(4),
        ..Default::default()
    };
    let id = state
        .core
        .chat_history()
        .create_conversation_with_settings(NewConversation {
            title: "t".to_owned(),
            model_id: Some(old),
            system_prompt: None,
            settings: Some(settings),
        })
        .await
        .unwrap();
    run(&state, LOCAL, "m1", id, on(used, "new-model")).await;
    let after = read(&state, id).await;
    assert_eq!(
        (after.model_id, named(&after)),
        (Some(used), Some("new-model"))
    );
    assert_eq!(after.settings.and_then(|s| s.max_iterations), Some(4));
}

/// A model the registry does not have clears the chat's id, which would
/// name another, and is named in the settings.
#[tokio::test]
async fn a_model_not_in_the_registry_is_named_in_the_settings_alone() {
    let (_dir, state) = state().await;
    let old = registered(&state, "old-model").await;
    let id = conversation(&state).await;
    run(&state, LOCAL, "m1", id, on(old, "old-model")).await;
    run(&state, LOCAL, "m2", id, on(4242, "loose-model")).await;
    let after = read(&state, id).await;
    assert_eq!((after.model_id, named(&after)), (None, Some("loose-model")));
}

/// A run on the far machine names nothing here.
#[tokio::test]
async fn a_run_on_the_far_machine_names_nothing() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (far, _) = prepared(finished_reply(), End::Finish);
    run(&state, LOCAL, "m1", id, far).await;
    let after = read(&state, id).await;
    assert_eq!((after.model_id, after.settings), (None, None));
}
