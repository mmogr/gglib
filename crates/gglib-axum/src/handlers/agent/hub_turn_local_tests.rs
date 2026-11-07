//! This machine's own turn on one of its chats, read against the hub's
//! record as a device's is, and different in three things only: a model
//! stored as the paired machine's runs there, every tool here is offered
//! unless the chat narrows it or its model calls none, and the turn may say
//! the chat's folder, which a device's never may.

use std::path::Path;

use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::{Machine, ModelCapabilities, ModelRef};
use gglib_core::ports::RunsPort as _;

use super::hub_turn_tests::{chat, device, refused, turn};
use super::{plan, start};
use crate::handlers::agent::device_tools::settle;
use crate::handlers::agent::run::with_code;
use crate::handlers::agent::run_fixture::{LOCAL, saved, settled, state};
use crate::handlers::agent::turn_fixture::{model, sent_in};
use crate::state::AppState;

/// The paired machine's model 3.
fn on_desk() -> ModelRef {
    ModelRef {
        machine: Machine::Paired {
            fingerprint: "0123456789ab".to_owned(),
        },
        id: 3,
    }
}

/// A chat with one question and its reply, that stored `model` as its own,
/// with `settings` besides.
async fn chat_on(state: &AppState, model: ModelRef, settings: ConversationSettings) -> i64 {
    let settings = ConversationSettings {
        model: Some(model),
        ..settings
    };
    chat(state, Some(settings)).await
}

/// One of this machine's catalogue models, by its id here.
fn here(id: i64) -> ModelRef {
    ModelRef {
        machine: Machine::Local,
        id,
    }
}

/// The tool filter this machine's turn on a chat of `model_id` with
/// `settings` is planned with.
async fn local_filter(
    state: &AppState,
    model_id: i64,
    settings: ConversationSettings,
) -> Option<Vec<String>> {
    let id = chat_on(state, here(model_id), settings).await;
    let planned = plan(state, &LOCAL, turn(id, "second"), None).await;
    planned.unwrap().chat.tool_filter
}

/// A chat that stored the paired machine's model runs there on this
/// machine's own turn: by that model, named as the chat names it, and
/// offered every tool here, since this catalogue says nothing of it.
/// A device's turn on the same chat is still refused.
#[tokio::test]
async fn a_chat_on_the_paired_machines_model_runs_there_on_this_machines_turn() {
    let (_dir, state) = state().await;
    let settings = ConversationSettings {
        model_name: Some("qwen3-desk".to_owned()),
        ..ConversationSettings::default()
    };
    let id = chat_on(&state, on_desk(), settings).await;

    let planned = plan(&state, &LOCAL, turn(id, "second"), None)
        .await
        .unwrap();

    assert_eq!(planned.chat.far, Some(on_desk()));
    assert_eq!(planned.model, "qwen3-desk");
    assert_eq!(planned.chat.tool_filter, None, "every tool");
    let device = plan(&state, &device("phone"), turn(id, "second"), None).await;
    assert_eq!(refused(device.map_err(with_code)), (409, "conflict"));
}

/// Started, such a turn goes to the paired machine and never loads a model
/// here: with no connection to it, the run ends `failed` as the tunnel
/// refuses it, not as a model that could not load, and nothing is written.
#[tokio::test]
async fn a_turn_on_the_paired_machines_model_never_loads_one_here() {
    let (_dir, state) = state().await;
    let id = chat_on(&state, on_desk(), ConversationSettings::default()).await;

    let created = start(&state, LOCAL, "m1", turn(id, "second"), None).await;

    assert!(created.unwrap().created);
    settled(&state).await;
    let run = state.runs.existing(&LOCAL, "m1").unwrap().unwrap();
    assert_eq!(
        run.model.as_deref(),
        Some("3"),
        "the id: the chat names no name"
    );
    let error = run.error.expect("it failed");
    assert_eq!(error.code, "conflict", "{}", error.message);
    assert_eq!(saved(&state, id).await.len(), 2, "no row was written");
}

/// A turn sent with Draw on the paired machine's model is refused before
/// anything runs: this machine cannot draw for another's model.
#[tokio::test]
async fn draw_on_the_paired_machines_model_is_refused_before_anything_runs() {
    let (_dir, state) = state().await;
    let id = chat_on(&state, on_desk(), ConversationSettings::default()).await;
    let drawn = HubTurn {
        draw: true,
        ..turn(id, "a fox")
    };

    let refusal = refused(start(&state, LOCAL, "m1", drawn, None).await);

    assert_eq!(refusal, (400, "drawing_unavailable"));
    assert!(state.runs.list(&LOCAL).runs.is_empty());
}

/// On a model that calls tools, this machine's turn is offered every tool
/// here when its chat names none, and the tools it names when it names
/// some; a device's turn on the same chat is planned with none.
#[tokio::test]
async fn this_machines_turn_is_offered_every_tool_or_those_its_chat_names() {
    let (_dir, state) = state().await;
    let model_id = model(&state, |_| {}).await;

    let every = local_filter(&state, model_id, ConversationSettings::default()).await;
    assert_eq!(every, None);

    let narrowed = ConversationSettings {
        tools: vec!["4:search".to_owned(), "read_file".to_owned()],
        ..ConversationSettings::default()
    };
    let named = local_filter(&state, model_id, narrowed.clone()).await;
    assert_eq!(
        named,
        Some(vec!["4:search".to_owned(), "read_file".to_owned()])
    );

    let id = chat_on(&state, here(model_id), narrowed).await;
    let phone = plan(&state, &device("phone"), turn(id, "second"), None).await;
    assert_eq!(phone.unwrap().chat.tool_filter, Some(Vec::new()));
}

/// The run's late step keeps what this machine's turn was planned with: it
/// reads no server open to devices, so every tool stays every tool.
#[tokio::test]
async fn the_late_step_keeps_this_machines_planned_tools() {
    let (_dir, state) = state().await;
    let model_id = model(&state, |_| {}).await;
    let id = chat_on(&state, here(model_id), ConversationSettings::default()).await;
    let planned = plan(&state, &LOCAL, turn(id, "hi"), None).await.unwrap();

    let settled = settle(&state, planned.chat, &planned.asked).await;

    assert_eq!(settled.tool_filter, None);
}

/// No tool at all for a chat that turned its tools off, or whose model the
/// catalogue says calls none, Draw or not: the loop adds the image tool
/// itself, after the folder, so it stays the only one.
#[tokio::test]
async fn no_tool_for_a_chat_with_tools_off_or_a_model_that_calls_none() {
    let (_dir, state) = state().await;
    let calls = model(&state, |_| {}).await;
    let off = ConversationSettings {
        no_tools: Some(true),
        tools: vec!["4:search".to_owned()],
        ..ConversationSettings::default()
    };
    assert_eq!(local_filter(&state, calls, off).await, Some(Vec::new()));

    let silent = model(&state, |m| {
        m.capabilities = ModelCapabilities::SUPPORTS_SYSTEM_ROLE;
    })
    .await;
    let plain = ConversationSettings::default();
    assert_eq!(local_filter(&state, silent, plain).await, Some(Vec::new()));
}

/// A model the catalogue does not hold is given its tools, as the page
/// gives one whose capability it does not know.
#[tokio::test]
async fn a_model_the_catalogue_does_not_hold_is_offered_every_tool() {
    let (_dir, state) = state().await;
    // Named only by its last reply: `qwen3-8b` is in no catalogue here.
    let id = chat(&state, None).await;
    let planned = plan(&state, &LOCAL, turn(id, "second"), None)
        .await
        .unwrap();
    assert_eq!(planned.model, "qwen3-8b");
    assert_eq!(planned.chat.tool_filter, None);
}

fn canonical(path: &Path) -> String {
    path.canonicalize().unwrap().to_str().unwrap().to_owned()
}

/// A folder this machine's turn says is the run's, by its canonical path,
/// and the chat's from then on; `""` forgets the chat's; saying nothing
/// reads the one the chat remembers and writes nothing.
#[tokio::test]
async fn this_machines_turn_settles_its_folder_as_the_pages_run_does() {
    let (_dir, state) = state().await;
    let place = tempfile::tempdir().unwrap();
    let folder = canonical(place.path());
    let id = chat(&state, None).await;

    let said = Some(place.path().to_str().unwrap().to_owned());
    let planned = plan(&state, &LOCAL, turn(id, "second"), said)
        .await
        .unwrap();
    assert_eq!(planned.folder, Some(folder.clone().into()));
    assert_eq!(
        planned.transcript.remember_folder,
        Some(Some(folder.clone()))
    );

    let settings = ConversationSettings {
        folder: Some(folder.clone()),
        ..ConversationSettings::default()
    };
    let id = chat(&state, Some(settings)).await;
    let kept = plan(&state, &LOCAL, turn(id, "second"), None)
        .await
        .unwrap();
    assert_eq!(kept.folder, Some(folder.into()));
    assert_eq!(kept.transcript.remember_folder, None);

    let forgot = plan(&state, &LOCAL, turn(id, "second"), Some(String::new())).await;
    let forgot = forgot.unwrap();
    assert_eq!(forgot.folder, None);
    assert_eq!(forgot.transcript.remember_folder, Some(None));
}

/// The folder's refusals come before anything runs: one said that is not
/// an absolute path to a folder, and one remembered that is gone.
#[tokio::test]
async fn a_folder_that_is_not_one_is_refused_before_anything_runs() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let relative = Some("not/absolute".to_owned());
    let refusal = refused(start(&state, LOCAL, "m1", turn(id, "second"), relative).await);
    assert_eq!(refusal, (400, "folder_unusable"));

    let settings = ConversationSettings {
        folder: Some("/no/such/folder/anywhere".to_owned()),
        ..ConversationSettings::default()
    };
    let gone = chat(&state, Some(settings)).await;
    let refusal = refused(start(&state, LOCAL, "m2", turn(gone, "second"), None).await);
    assert_eq!(refusal, (409, "folder_missing"));
    assert!(state.runs.list(&LOCAL).runs.is_empty());
    assert_eq!(saved(&state, id).await.len(), 2);
}

/// A device's turn never has a folder: one it says is refused, and the one
/// its chat remembers is not its.
#[tokio::test]
async fn a_devices_turn_says_no_folder_and_reads_none() {
    let (_dir, state) = state().await;
    let place = tempfile::tempdir().unwrap();
    let id = chat(&state, None).await;
    let said = Some(place.path().to_str().unwrap().to_owned());
    let refusal = refused(start(&state, device("phone"), "d1", turn(id, "second"), said).await);
    assert_eq!(refusal, (400, "invalid_request"));

    let settings = ConversationSettings {
        folder: Some(canonical(place.path())),
        ..ConversationSettings::default()
    };
    let id = chat(&state, Some(settings)).await;
    let phone = plan(&state, &device("phone"), turn(id, "second"), None).await;
    let phone = phone.unwrap();
    assert_eq!(
        (phone.folder, phone.transcript.remember_folder),
        (None, None)
    );
}

/// The folder this machine's turn is planned with is where its loop's file
/// tools read.
#[tokio::test]
async fn this_machines_turn_is_offered_the_file_tools_in_its_folder() {
    let (_dir, state) = state().await;
    let model_id = model(&state, |_| {}).await;
    let place = tempfile::tempdir().unwrap();
    let id = chat_on(&state, here(model_id), ConversationSettings::default()).await;
    let said = Some(place.path().to_str().unwrap().to_owned());

    let planned = plan(&state, &LOCAL, turn(id, "hi"), said).await.unwrap();
    let body = sent_in(&state, model_id, planned.chat, planned.folder).await;

    let read_file = body["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["function"]["name"] == "builtin:read_file")
        .expect("read_file is offered");
    let told = read_file["function"]["description"].as_str().unwrap();
    assert!(told.contains(&canonical(place.path())), "{told}");
}

/// A phone's turn on a chat that chose the paired machine's model and has
/// no saved message runs on this machine's, here the hub's default: the
/// chat has run nowhere yet. A message saved, the same turn is refused.
#[tokio::test]
async fn a_phone_turn_on_a_new_chat_that_chose_the_paired_model_runs_here() {
    let (_dir, state) = state().await;
    let default = model(&state, |_| {}).await;
    let mut settings = state.core.settings().get().await.unwrap();
    settings.default_model_id = Some(default);
    state.core.settings().save(&settings).await.unwrap();
    let chosen = ConversationSettings {
        model_name: Some("qwen3-desk".to_owned()),
        model: Some(on_desk()),
        ..ConversationSettings::default()
    };
    let id = state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            settings: Some(chosen.clone()),
            ..NewConversation::default()
        })
        .await
        .unwrap();

    let phone = plan(&state, &device("phone"), turn(id, "hi"), None).await;
    let phone = phone.unwrap();
    assert_eq!((phone.model, phone.chat.far), (default.to_string(), None));

    let ran = chat(&state, Some(chosen)).await;
    let phone = plan(&state, &device("phone"), turn(ran, "hi"), None).await;
    assert_eq!(refused(phone.map_err(with_code)), (409, "conflict"));
}
