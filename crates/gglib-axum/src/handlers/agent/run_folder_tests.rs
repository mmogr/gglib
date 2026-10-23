//! This machine's own run and its conversation's folder: what a run says of
//! it, what the conversation remembers, what is refused before anything is
//! written, and where gglib's own file tools are offered, by which root.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::RunsPort as _;
use serde_json::{Value, json};

use super::AgentChatRequest;
use super::compose::{context_tests::LaunchedAt, take_permit};
use super::dto::AgentRunRequest;
use super::launch::{Transcript, launch};
use super::run::{create_run, plan};
use super::run_fixture::{End, LOCAL, finished_reply, prepared, saved, settled, state};
use super::turn_fixture::{TOOL, hi, model, sent_by_device, sent_in};
use crate::error::HttpError;
use crate::state::AppState;

/// A run's body saved to `conversation`, with `more` keys beside the chat
/// request's.
fn body(conversation: Option<i64>, more: &Value) -> Value {
    let mut body = json!({
        "port": 9000,
        "messages": [{ "role": "user", "content": "hi" }],
        "conversation_id": conversation,
    });
    let more = more.as_object().expect("keys").clone();
    body.as_object_mut().expect("an object").extend(more);
    body
}

/// What a run that says `more` is planned with: its chat request, what it
/// writes once it starts, and its folder.
async fn planned(
    state: &AppState,
    conversation: Option<i64>,
    more: &Value,
) -> Result<(AgentChatRequest, Transcript, Option<PathBuf>), HttpError> {
    let request: AgentRunRequest = serde_json::from_value(body(conversation, more)).unwrap();
    plan(state, request).await
}

/// Plan the run `more` says and run it to its end as run `id`: its folder.
async fn run(state: &AppState, id: &str, conversation: i64, more: &Value) -> Option<PathBuf> {
    let (_, transcript, folder) = planned(state, Some(conversation), more).await.unwrap();
    let (p, _) = prepared(finished_reply(), End::Finish);
    let created = launch(
        state,
        id,
        LOCAL,
        transcript,
        p,
        take_permit(state).expect("a slot"),
    )
    .await
    .unwrap();
    assert!(created.created, "{id}");
    settled(state).await;
    folder
}

/// The folder conversation `id` remembers.
async fn remembered(state: &AppState, id: i64) -> Option<String> {
    let history = state.core.chat_history();
    let conversation = history.get_conversation(id).await.unwrap().unwrap();
    conversation.settings.and_then(|settings| settings.folder)
}

/// A conversation whose settings are `settings`.
async fn chat_with(state: &AppState, settings: ConversationSettings) -> i64 {
    state
        .core
        .chat_history()
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: None,
            system_prompt: None,
            settings: Some(settings),
        })
        .await
        .unwrap()
}

/// A conversation that remembers `folder`, as written.
async fn remembering(state: &AppState, folder: &str) -> i64 {
    let settings = ConversationSettings {
        folder: Some(folder.to_owned()),
        ..ConversationSettings::default()
    };
    chat_with(state, settings).await
}

/// The door's refusal of a run that says `more`: its status, its code and
/// its message.
async fn refused(state: &AppState, id: i64, more: &Value) -> (u16, &'static str, String) {
    match create_run(state, "r1", body(Some(id), more)).await {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => (status.as_u16(), code, message),
        other => panic!("not a coded refusal: {other:?}"),
    }
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

fn shown(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

/// A folder said through a link is the folder it leads to: that is what the
/// run reads, what the conversation remembers, and what the next run,
/// which says nothing, reads again without writing.
#[cfg(unix)]
#[tokio::test]
async fn a_said_folder_is_remembered_by_its_canonical_path_and_read_again() {
    let (_dir, state) = state().await;
    let place = tempfile::tempdir().unwrap();
    let project = place.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let link = place.path().join("link");
    std::os::unix::fs::symlink(&project, &link).unwrap();
    let project = canonical(&project);
    let id = super::run_fixture::conversation(&state).await;

    let said = run(&state, "r1", id, &json!({ "folder": shown(&link) })).await;
    assert_eq!(said.as_deref(), Some(project.as_path()));
    assert_eq!(remembered(&state, id).await, Some(shown(&project)));

    let (_, transcript, next) = planned(&state, Some(id), &json!({})).await.unwrap();
    assert_eq!(next.as_deref(), Some(project.as_path()));
    assert_eq!(transcript.remember_folder, None, "nothing to write");
}

/// `""` forgets the folder without checking it, so a folder that is gone
/// can be cleared, and the next run has none.
#[tokio::test]
async fn the_empty_word_forgets_a_folder_even_one_that_is_gone() {
    let (_dir, state) = state().await;
    let id = remembering(&state, "/nonexistent/gglib-folder").await;

    assert_eq!(run(&state, "r1", id, &json!({ "folder": "" })).await, None);
    assert_eq!(remembered(&state, id).await, None);
    assert_eq!(run(&state, "r2", id, &json!({})).await, None);
}

/// A said path that is relative, names nothing, or names a file is refused
/// 400 before a run is made: no message is saved and the folder the
/// conversation remembers stays.
#[tokio::test]
async fn a_said_path_that_is_no_folder_is_refused_400_and_writes_nothing() {
    let (_dir, state) = state().await;
    let place = tempfile::tempdir().unwrap();
    let kept = shown(&canonical(place.path()));
    let file = place.path().join("notes.txt");
    std::fs::write(&file, "x").unwrap();
    let id = remembering(&state, &kept).await;

    for path in [
        "project".to_owned(),
        "/nonexistent/gglib-folder".to_owned(),
        shown(&file),
    ] {
        let (status, code, message) = refused(&state, id, &json!({ "folder": path })).await;
        assert_eq!((status, code), (400, "folder_unusable"), "{path}");
        assert!(message.contains(&path), "{path}: {message}");
    }
    assert!(state.runs.get(&LOCAL, "r1").is_err(), "no run was made");
    assert!(saved(&state, id).await.is_empty(), "no message was saved");
    assert_eq!(remembered(&state, id).await, Some(kept));
}

/// A remembered folder that is gone is refused 409 when the run says
/// nothing of it, with nothing written. Saying another folder, or `""`, is
/// how the chat goes on: neither is refused for the folder that is gone.
#[tokio::test]
async fn a_remembered_folder_that_is_gone_is_refused_409() {
    let (_dir, state) = state().await;
    let place = tempfile::tempdir().unwrap();
    let gone = place.path().join("gone");
    std::fs::create_dir(&gone).unwrap();
    let gone = shown(&canonical(&gone));
    std::fs::remove_dir(&gone).unwrap();
    let id = remembering(&state, &gone).await;

    let (status, code, message) = refused(&state, id, &json!({})).await;
    assert_eq!((status, code), (409, "folder_missing"));
    assert!(message.contains(&gone), "{message}");
    assert!(state.runs.get(&LOCAL, "r1").is_err(), "no run was made");
    assert!(saved(&state, id).await.is_empty(), "no message was saved");
    assert_eq!(remembered(&state, id).await, Some(gone));

    for another in [shown(place.path()), String::new()] {
        let said = json!({ "folder": &another });
        assert!(planned(&state, Some(id), &said).await.is_ok(), "{another}");
    }
}

/// Without a conversation a said folder is this run's alone, and `""` is
/// the same as saying nothing.
#[tokio::test]
async fn a_run_with_no_conversation_reads_its_folder_and_remembers_nothing() {
    let (_dir, state) = state().await;
    let place = tempfile::tempdir().unwrap();
    let said = json!({ "folder": shown(place.path()) });

    let (_, transcript, folder) = planned(&state, None, &said).await.unwrap();
    assert_eq!(folder, Some(canonical(place.path())));
    assert_eq!(transcript.remember_folder, None);
    let (_, transcript, folder) = planned(&state, None, &json!({ "folder": "" }))
        .await
        .unwrap();
    assert_eq!((folder, transcript.remember_folder), (None, None));
}

/// The description of `builtin:read_file` in a request, if it offers it.
fn read_file_description(body: &Value) -> Option<String> {
    let tools = body["tools"].as_array()?;
    let tool = tools
        .iter()
        .find(|tool| tool["function"]["name"] == "builtin:read_file")?;
    tool["function"]["description"].as_str().map(str::to_owned)
}

/// Two runs at once, each with its own folder, are each told their own:
/// the root is the run's, not the process's.
#[tokio::test]
async fn two_runs_at_once_name_their_own_folders() {
    let (_dir, state) = state().await;
    let model_id = model(&state, |_| {}).await;
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let page = |folder: &Path| json!({ "tool_filter": [TOOL], "folder": shown(folder) });
    let (chat_a, _, in_a) = planned(&state, None, &page(a.path())).await.unwrap();
    let (chat_b, _, in_b) = planned(&state, None, &page(b.path())).await.unwrap();

    let (sent_a, sent_b) = tokio::join!(
        sent_in(&state, model_id, chat_a, in_a),
        sent_in(&state, model_id, chat_b, in_b),
    );

    let (a, b) = (shown(&canonical(a.path())), shown(&canonical(b.path())));
    let told_a = read_file_description(&sent_a).expect("run a is offered read_file");
    let told_b = read_file_description(&sent_b).expect("run b is offered read_file");
    assert!(told_a.contains(&a) && !told_a.contains(&b), "{told_a}");
    assert!(told_b.contains(&b) && !told_b.contains(&a), "{told_b}");
}

/// A paired device's turn on a chat that remembers a folder, planned and
/// prepared as a device's is (`hub_turn::plan`, then `held_for_device`, the
/// step of `late_on` after its model is found), is offered none of the file
/// tools, even with every tool open to it, while the page's run on the same
/// chat reads that folder.
#[tokio::test]
async fn a_devices_turn_on_a_chat_with_a_folder_is_offered_no_file_tool() {
    let (_dir, mut state) = state().await;
    // Holding the model a turn asks for, as `held_for_device` takes it.
    Arc::get_mut(&mut state).expect("not yet shared").runtime =
        Arc::new(LaunchedAt(4_096, Mutex::new(None)));
    let model_id = model(&state, |_| {}).await;
    let place = tempfile::tempdir().unwrap();
    let folder = shown(&canonical(place.path()));
    let settings = ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Local,
            id: model_id,
        }),
        folder: Some(folder.clone()),
        ..ConversationSettings::default()
    };
    let id = chat_with(&state, settings).await;

    let mut phone = super::hub_turn::plan(&state, hi(id)).await.unwrap().chat;
    // Every tool, as no device's turn is given: what keeps the file tools
    // out is the folder, not the filter.
    phone.tool_filter = None;
    let device = sent_by_device(&state, model_id, phone).await;
    let names: Vec<&str> = device["tools"]
        .as_array()
        .expect("the turn is offered tools")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    assert!(names.contains(&TOOL), "{names:?}");
    assert!(!names.contains(&"builtin:read_file"), "{names:?}");

    let (page, _, here) = planned(&state, Some(id), &json!({})).await.unwrap();
    let page = sent_in(&state, model_id, page, here).await;
    let told = read_file_description(&page).expect("the page's run is offered read_file");
    assert!(told.contains(&folder), "{told}");
}
