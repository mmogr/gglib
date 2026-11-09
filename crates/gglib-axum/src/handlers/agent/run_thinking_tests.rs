//! This machine's own turn and its chat's Thinking choice: the rule a
//! device's turn is read by, and what a run writes of it once it starts.

use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::attachment::AttachmentId;
use gglib_core::domain::chat::{Conversation, ConversationSettings, NewConversation};
use gglib_core::domain::hub_chats::HubTurn;
use gglib_core::domain::{Machine, ModelRef, Thinking};
use serde_json::{Value, json};

use super::compose::{Prepared, take_permit};
use super::hub_turn::{plan, start_local};
use super::launch::Transcript;
use super::run_fixture::launch;
use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, prepared, qwen, saving, settled, state,
};
use crate::error::HttpError;
use crate::state::AppState;
use gglib_core::ports::RunsPort as _;

async fn read(state: &AppState, id: i64) -> Conversation {
    let history = state.core.chat_history();
    history.get_conversation(id).await.unwrap().unwrap()
}

/// What conversation `id` remembers of thinking.
pub(super) async fn remembered(state: &AppState, id: i64) -> Option<Thinking> {
    read(state, id).await.settings.and_then(|s| s.thinking)
}

/// A chat on this machine's `qwen`, by its id, that remembers `thinking`.
async fn chat(state: &AppState, thinking: Option<Thinking>) -> i64 {
    let settings = ConversationSettings {
        model: Some(qwen(state).await),
        model_name: Some("qwen".to_owned()),
        thinking,
        ..ConversationSettings::default()
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

/// This machine's turn on chat `id`, saying `thinking`.
fn turn(id: i64, thinking: Option<Thinking>) -> HubTurn {
    HubTurn {
        conversation_id: id,
        content: "hi".to_owned(),
        images: Vec::new(),
        thinking,
        answer_saved: false,
        draw: false,
        image_model: None,
    }
}

/// Read this machine's turn as the door does and run it to its end as run
/// `id`: the thinking budget it ran with.
async fn run(state: &AppState, id: &str, chat: i64, said: Option<Thinking>) -> Option<i32> {
    let planned = plan(state, &LOCAL, turn(chat, said), None).await.unwrap();
    start(state, id, planned.transcript).unwrap();
    settled(state).await;
    planned.chat.reasoning_budget_tokens
}

fn start(state: &AppState, id: &str, transcript: Transcript) -> Result<bool, HttpError> {
    let (p, _) = prepared(finished_reply(), End::Finish);
    start_on(state, id, transcript, p)
}

/// Launch `p` as this machine's run `id`, saved as `transcript` says:
/// whether a run was made.
fn start_on(
    state: &AppState,
    id: &str,
    transcript: Transcript,
    p: Prepared,
) -> Result<bool, HttpError> {
    let free = take_permit(state).expect("a free slot");
    launch(state, id, LOCAL, transcript, p, free).map(|created| created.created)
}

#[tokio::test]
async fn this_machines_own_turn_remembers_the_same_way() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;

    let off = run(&state, "r1", id, Some(Thinking::Off)).await;
    assert_eq!(off, Some(0));
    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));
    let next = run(&state, "r2", id, None).await;
    assert_eq!(next, Some(0), "the next turn says nothing and is off");

    let back = run(&state, "r3", id, Some(Thinking::Default)).await;
    assert_eq!(back, None);
    assert_eq!(remembered(&state, id).await, None);
}

/// A chat switched off stays off, and stays remembered, until a turn says
/// `default`: the page sends no budget of its own.
#[tokio::test]
async fn a_remembered_off_holds_until_a_turn_says_default() {
    let (_dir, state) = state().await;
    let id = chat(&state, Some(Thinking::Off)).await;
    assert_eq!(run(&state, "r1", id, None).await, Some(0));
    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));
    assert_eq!(run(&state, "r2", id, Some(Thinking::Default)).await, None);
    assert_eq!(remembered(&state, id).await, None);
    assert_eq!(run(&state, "r3", id, None).await, None);
}

/// The door's refusal of the turn `body`: its status, its code and its
/// message.
async fn refused(state: &AppState, body: Value) -> (u16, &'static str, String) {
    match start_local(state, "r1", body).await {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => (status.as_u16(), code, message),
        other => panic!("not a coded refusal: {other:?}"),
    }
}

/// A word that is neither is not a turn: the door refuses it as a body,
/// before a chat is read or a run is made. `off` is read, and refused for
/// what the chat lacks (here, a model), not as a body.
#[tokio::test]
async fn an_unknown_thinking_value_is_refused_400() {
    const NOT_A_BODY: &str = "a turn's body is";
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let body = |word: Value| json!({ "conversation_id": id, "content": "hi", "thinking": word });

    for word in [json!("on"), json!("OFF"), json!(0)] {
        let (status, code, message) = refused(&state, body(word.clone())).await;
        assert_eq!((status, code), (400, "invalid_request"), "{word}");
        assert!(message.starts_with(NOT_A_BODY), "{word}: {message}");
    }
    let (status, code, message) = refused(&state, body(json!("off"))).await;
    assert_eq!((status, code), (422, "no_model"), "{message}");

    assert_eq!(remembered(&state, id).await, None);
    assert!(state.runs.get(&LOCAL, "r1").is_err(), "no run was made");
}

/// The launch a real run makes, which the runs above do not: their loop
/// names no model of this machine's, so the launch writes the choice and no
/// model. A run on a registered model that says `off` leaves the chat with
/// its model and `off`, and a plain run after it keeps both.
#[tokio::test]
async fn a_run_on_a_registered_model_writes_its_model_and_the_choice_and_keeps_both() {
    let (_dir, state) = state().await;
    let model = gglib_core::domain::NewModel::new(
        "qwen".to_owned(),
        std::path::PathBuf::from("/models/qwen.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    let model = state.core.models().add(model).await.unwrap().id;
    let id = chat(&state, None).await;
    let here = ModelRef {
        machine: Machine::Local,
        id: model,
    };
    for (run_id, said) in [("r1", Some(Thinking::Off)), ("r2", None)] {
        let planned = plan(&state, &LOCAL, turn(id, said), None).await.unwrap();
        let (mut p, _) = prepared(finished_reply(), End::Finish);
        p.local_model = Some((19_555, model));
        let started = start_on(&state, run_id, planned.transcript, p);
        assert!(started.unwrap(), "{run_id}");
        settled(&state).await;

        let after = read(&state, id).await;
        let settings = after.settings.expect("the run wrote its settings");
        assert_eq!(after.model_id, Some(model), "{run_id}");
        assert_eq!(settings.model, Some(here.clone()), "{run_id}");
        assert_eq!(settings.model_name.as_deref(), Some("qwen"), "{run_id}");
        assert_eq!(settings.thinking, Some(Thinking::Off), "{run_id}");
    }
}

/// A run refused after its id is reserved, here for an image its message
/// names that is not stored, ends `failed` with that code, and has written
/// nothing of thinking either.
#[tokio::test]
async fn a_run_refused_at_its_launch_remembers_nothing() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let transcript = Transcript {
        remember: Some(Some(Thinking::Off)),
        ..saving(id)
    };
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.messages = vec![AgentMessage::User {
        content: "Look.".to_owned(),
        images: vec![AttachmentId::of(b"never stored")],
    }];
    assert!(start_on(&state, "r1", transcript, p).unwrap());
    settled(&state).await;

    let ended = state.runs.get(&LOCAL, "r1").unwrap();
    assert_eq!(ended.status, gglib_core::domain::runs::RunStatus::Failed);
    let code = ended.error.map(|e| e.code);
    assert_eq!(code.as_deref(), Some("attachment_not_found"));
    assert_eq!(remembered(&state, id).await, None);
}

/// A conversation with no settings that is told to forget is left with
/// none, not given an empty settings object.
#[tokio::test]
async fn forgetting_what_was_never_remembered_writes_nothing() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let forget = Transcript {
        remember: Some(None),
        ..saving(id)
    };
    assert!(start(&state, "r1", forget).unwrap());
    settled(&state).await;
    assert_eq!(read(&state, id).await.settings, None);
}
