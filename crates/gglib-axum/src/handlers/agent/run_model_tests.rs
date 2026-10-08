//! A run names the model it uses on its conversation, from either door, so
//! the chat's next turn runs on it.

use gglib_core::domain::chat::{Conversation, ConversationSettings, NewConversation};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::RunScope;

use super::compose::{Prepared, take_permit};
use super::hub_model::choose;
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
    let settings = ConversationSettings {
        model_name: Some("old-model".to_owned()),
        max_iterations: Some(4),
        ..Default::default()
    };
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
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

/// A model the registry does not have clears the chat's id and its stored
/// model, which would name another, and is named in the settings.
#[tokio::test]
async fn a_model_not_in_the_registry_is_named_in_the_settings_alone() {
    let (_dir, state) = state().await;
    let old = registered(&state, "old-model").await;
    let id = conversation(&state).await;
    run(&state, LOCAL, "m1", id, on(old, "old-model")).await;
    run(&state, LOCAL, "m2", id, on(4242, "loose-model")).await;
    let after = read(&state, id).await;
    assert_eq!((after.model_id, named(&after)), (None, Some("loose-model")));
    assert_eq!(after.settings.and_then(|s| s.model), None);
}

const fn here(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

/// Model `id` of the paired machine.
fn there(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Paired {
            fingerprint: "0a1b2c3d4e5f".to_owned(),
        },
        id,
    }
}

/// A run on the paired machine's model `id`, which that machine calls `name`.
fn far_run(id: i64, name: &str) -> Prepared {
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.far_model = Some(there(id));
    name.clone_into(&mut p.made_by.model);
    p
}

fn stored(conversation: &Conversation) -> Option<ModelRef> {
    conversation.settings.as_ref()?.model.clone()
}

/// A run on the paired machine names its model by that machine, never as
/// an id of this one, so the chat's next device turn is refused rather than
/// run here on a model that shares its name or its number.
#[tokio::test]
async fn a_far_run_names_its_model_by_the_paired_machine() {
    let (_dir, state) = state().await;
    registered(&state, "qwen3").await;
    let id = conversation(&state).await;

    run(&state, LOCAL, "m1", id, far_run(1, "qwen3")).await;

    let after = read(&state, id).await;
    assert_eq!(
        (after.model_id, stored(&after), named(&after)),
        (None, Some(there(1)), Some("qwen3"))
    );
    assert!(
        choose(&state, &after, &[], &[]).await.is_err(),
        "the paired machine's chat ran here"
    );
}

/// A conversation the page made for a far model keeps that model across a
/// run on it, and its other settings with it.
#[tokio::test]
async fn a_far_run_keeps_the_chats_paired_ref() {
    let (_dir, state) = state().await;
    let settings = ConversationSettings {
        model: Some(there(7)),
        max_iterations: Some(4),
        ..Default::default()
    };
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(settings),
        })
        .await
        .unwrap();

    run(&state, LOCAL, "m1", id, far_run(7, "qwen3")).await;

    let after = read(&state, id).await;
    assert_eq!((after.model_id, stored(&after)), (None, Some(there(7))));
    assert_eq!(after.settings.and_then(|s| s.max_iterations), Some(4));
}

/// A run on neither machine's model names nothing.
#[tokio::test]
async fn a_run_on_no_model_names_nothing() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (neither, _) = prepared(finished_reply(), End::Finish);
    run(&state, LOCAL, "m1", id, neither).await;
    let after = read(&state, id).await;
    assert_eq!((after.model_id, after.settings), (None, None));
}

/// A chat the CLI started on one model, which stored it by id, and a run
/// here then continued on another: the stored model is the one the run
/// used, so the chat's next turn runs on it, and its id, model and name
/// all say the same model.
#[tokio::test]
async fn a_run_on_another_model_replaces_the_stored_model_too() {
    let (_dir, state) = state().await;
    let old = registered(&state, "old-model").await;
    let used = registered(&state, "new-model").await;
    let settings = ConversationSettings {
        model_name: Some("old-model".to_owned()),
        model: Some(here(old)),
        ..Default::default()
    };
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(settings),
        })
        .await
        .unwrap();

    run(&state, LOCAL, "m1", id, on(used, "new-model")).await;

    let after = read(&state, id).await;
    let stored = after.settings.as_ref().and_then(|s| s.model.clone());
    assert_eq!(
        (after.model_id, stored, named(&after)),
        (Some(used), Some(here(used)), Some("new-model"))
    );
    let next = choose(&state, &after, &[], &[]).await.unwrap();
    assert_eq!(
        next,
        used.to_string(),
        "the next turn runs on the new model"
    );
}
