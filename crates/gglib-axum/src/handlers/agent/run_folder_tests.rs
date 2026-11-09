//! This machine's own turn and its chat's folder: what a turn says of
//! it, what the conversation remembers, what is refused before anything is
//! written, and where gglib's own file tools are offered, by which root.

use std::path::{Path, PathBuf};

use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::{Machine, ModelRef};
use gglib_core::ports::RunsPort as _;
use serde_json::{Value, json};

use super::compose::take_permit;
use super::hub_turn::{Plan, plan, start_local};
use super::run_fixture::launch;
use super::run_fixture::{
    End, LOCAL, device, finished_reply, prepared, qwen, saved, settled, state,
};
use super::turn_fixture::{TOOL, hi, model, sent, sent_in};
use crate::error::HttpError;
use crate::state::AppState;

/// This machine's turn on `conversation` that says `folder`.
fn saying(conversation: i64, folder: Option<&str>) -> Value {
    let mut body = json!({ "conversation_id": conversation, "content": "hi" });
    if let Some(folder) = folder {
        body["folder"] = json!(folder);
    }
    body
}

/// What this machine's turn that says `folder` is planned with: its chat
/// request, what it writes once it starts, and its folder.
async fn planned(
    state: &AppState,
    conversation: i64,
    folder: Option<&str>,
) -> Result<Plan, HttpError> {
    let turn = hi(conversation);
    plan(state, &LOCAL, turn, folder.map(str::to_owned)).await
}

/// Plan the turn that says `folder` and run it to its end as run `id`: its
/// folder.
async fn run(
    state: &AppState,
    id: &str,
    conversation: i64,
    folder: Option<&str>,
) -> Option<PathBuf> {
    let planned = planned(state, conversation, folder).await.unwrap();
    let (p, _) = prepared(finished_reply(), End::Finish);
    let created = launch(
        state,
        id,
        LOCAL,
        planned.transcript,
        p,
        take_permit(state).expect("a slot"),
    )
    .unwrap();
    assert!(created.created, "{id}");
    settled(state).await;
    planned.folder
}

/// The folder conversation `id` remembers.
async fn remembered(state: &AppState, id: i64) -> Option<String> {
    let history = state.core.chat_history();
    let conversation = history.get_conversation(id).await.unwrap().unwrap();
    conversation.settings.and_then(|settings| settings.folder)
}

/// A conversation whose settings are `settings`, on this machine's `qwen`
/// by its id unless they store a model.
async fn chat_with(state: &AppState, settings: ConversationSettings) -> i64 {
    let model = match settings.model {
        Some(model) => model,
        None => qwen(state).await,
    };
    let settings = ConversationSettings {
        model: Some(model),
        model_name: settings.model_name.or_else(|| Some("qwen".to_owned())),
        ..settings
    };
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

/// The door's refusal of this machine's turn that says `folder`: its
/// status, its code and its message.
async fn refused(state: &AppState, id: i64, folder: Option<&str>) -> (u16, &'static str, String) {
    match start_local(state, "r1", saying(id, folder)).await {
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
    let id = chat_with(&state, ConversationSettings::default()).await;

    let said = run(&state, "r1", id, Some(&shown(&link))).await;
    assert_eq!(said.as_deref(), Some(project.as_path()));
    assert_eq!(remembered(&state, id).await, Some(shown(&project)));

    let next = planned(&state, id, None).await.unwrap();
    assert_eq!(next.folder.as_deref(), Some(project.as_path()));
    assert_eq!(next.transcript.remember_folder, None, "nothing to write");
}

/// `""` forgets the folder without checking it, so a folder that is gone
/// can be cleared, and the next run has none.
#[tokio::test]
async fn the_empty_word_forgets_a_folder_even_one_that_is_gone() {
    let (_dir, state) = state().await;
    let id = remembering(&state, "/nonexistent/gglib-folder").await;

    assert_eq!(run(&state, "r1", id, Some("")).await, None);
    assert_eq!(remembered(&state, id).await, None);
    assert_eq!(run(&state, "r2", id, None).await, None);
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
        let (status, code, message) = refused(&state, id, Some(&path)).await;
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

    let (status, code, message) = refused(&state, id, None).await;
    assert_eq!((status, code), (409, "folder_missing"));
    assert!(message.contains(&gone), "{message}");
    assert!(state.runs.get(&LOCAL, "r1").is_err(), "no run was made");
    assert!(saved(&state, id).await.is_empty(), "no message was saved");
    assert_eq!(remembered(&state, id).await, Some(gone));

    for another in [shown(place.path()), String::new()] {
        let said = planned(&state, id, Some(&another)).await;
        assert!(said.is_ok(), "{another}");
    }
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
    let on_tool = ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Local,
            id: model_id,
        }),
        tools: vec![TOOL.to_owned()],
        ..ConversationSettings::default()
    };
    let (one, two) = (
        chat_with(&state, on_tool.clone()).await,
        chat_with(&state, on_tool).await,
    );
    let in_a = planned(&state, one, Some(&shown(a.path()))).await.unwrap();
    let in_b = planned(&state, two, Some(&shown(b.path()))).await.unwrap();

    let (sent_a, sent_b) = tokio::join!(
        sent_in(&state, model_id, in_a.chat, in_a.folder),
        sent_in(&state, model_id, in_b.chat, in_b.folder),
    );

    let (a, b) = (shown(&canonical(a.path())), shown(&canonical(b.path())));
    let told_a = read_file_description(&sent_a).expect("run a is offered read_file");
    let told_b = read_file_description(&sent_b).expect("run b is offered read_file");
    assert!(told_a.contains(&a) && !told_a.contains(&b), "{told_a}");
    assert!(told_b.contains(&b) && !told_b.contains(&a), "{told_b}");
}

/// A paired device's turn on a chat that remembers a folder, planned and
/// prepared as a device's is (`hub_turn::plan`, whose folder for it is none,
/// then through this machine's endpoint, as `late_on` sends it once its
/// model is loaded), is offered none of the file tools, even with every tool
/// open to it, while this machine's turn on the same chat reads that folder.
#[tokio::test]
async fn a_devices_turn_on_a_chat_with_a_folder_is_offered_no_file_tool() {
    let (_dir, state) = state().await;
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

    let mut phone = super::hub_turn::plan(&state, &device("phone"), hi(id), None)
        .await
        .unwrap()
        .chat;
    // Every tool, as no device's turn is given: what keeps the file tools
    // out is the folder, not the filter.
    phone.tool_filter = None;
    let device = sent(&state, model_id, phone).await;
    let names: Vec<&str> = device["tools"]
        .as_array()
        .expect("the turn is offered tools")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str())
        .collect();
    assert!(names.contains(&TOOL), "{names:?}");
    assert!(!names.contains(&"builtin:read_file"), "{names:?}");

    let here = planned(&state, id, None).await.unwrap();
    let page = sent_in(&state, model_id, here.chat, here.folder).await;
    let told = read_file_description(&page).expect("this machine's turn is offered read_file");
    assert!(told.contains(&folder), "{told}");
}
