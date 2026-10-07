//! This machine's own run and its conversation's Thinking choice: the rule
//! a device's turn is read by, over a request that may carry a budget of
//! its own, and what a run writes of it once it starts.

use gglib_core::domain::chat::{Conversation, ConversationSettings, NewConversation};
use gglib_core::domain::{Machine, ModelRef, Thinking};
use serde_json::{Value, json};

use super::compose::{Prepared, take_permit};
use super::dto::AgentRunRequest;
use super::launch::{Transcript, launch};
use super::run::{create_run, plan};
use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, prepared, saving, settled, state,
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

/// A conversation that remembers `off`.
async fn switched_off(state: &AppState) -> i64 {
    let settings = ConversationSettings {
        thinking: Some(Thinking::Off),
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

/// Read a run's request as the door does and run it to its end as run
/// `id`: the thinking budget it ran with.
async fn run(state: &AppState, id: &str, conversation: Option<i64>, more: Value) -> Option<i32> {
    let request: AgentRunRequest = serde_json::from_value(body(conversation, &more)).unwrap();
    let (chat, transcript) = plan(state, request).await.unwrap();
    start(state, id, transcript).await.unwrap();
    settled(state).await;
    chat.reasoning_budget_tokens
}

async fn start(state: &AppState, id: &str, transcript: Transcript) -> Result<bool, HttpError> {
    let (p, _) = prepared(finished_reply(), End::Finish);
    start_on(state, id, transcript, p).await
}

/// Launch `p` as this machine's run `id`, saved as `transcript` says:
/// whether a run was made.
async fn start_on(
    state: &AppState,
    id: &str,
    transcript: Transcript,
    p: Prepared,
) -> Result<bool, HttpError> {
    let free = take_permit(state).expect("a free slot");
    launch(state, id, LOCAL, transcript, p, free)
        .await
        .map(|created| created.created)
}

#[tokio::test]
async fn this_machines_own_run_remembers_the_same_way() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;

    let off = run(&state, "r1", Some(id), json!({ "thinking": "off" })).await;
    assert_eq!(off, Some(0));
    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));
    let next = run(&state, "r2", Some(id), json!({})).await;
    assert_eq!(next, Some(0), "the next run says nothing and is off");

    let back = run(&state, "r3", Some(id), json!({ "thinking": "default" })).await;
    assert_eq!(back, None);
    assert_eq!(remembered(&state, id).await, None);
}

/// The page sends its device-wide budget with every message; a chat
/// switched off stays off, and stays remembered.
#[tokio::test]
async fn a_remembered_off_beats_a_requests_own_budget() {
    let (_dir, state) = state().await;
    let id = switched_off(&state).await;
    let own = json!({ "reasoning_budget_tokens": 4096 });
    assert_eq!(run(&state, "r1", Some(id), own).await, Some(0));
    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));
}

#[tokio::test]
async fn a_run_that_says_default_forgets_and_uses_the_requests_own_budget() {
    let (_dir, state) = state().await;
    let id = switched_off(&state).await;
    let said = json!({ "thinking": "default", "reasoning_budget_tokens": 4096 });
    assert_eq!(run(&state, "r1", Some(id), said).await, Some(4096));
    assert_eq!(remembered(&state, id).await, None);
    let own = json!({ "reasoning_budget_tokens": 2048 });
    assert_eq!(run(&state, "r2", Some(id), own).await, Some(2048));
}

/// A request's own `0` stops that run's thinking, and its effort level is
/// that run's: neither is written, and the next run thinks.
#[tokio::test]
async fn a_requests_own_budget_is_never_remembered() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let own = json!({ "reasoning_budget_tokens": 0, "reasoning_effort": "low" });
    assert_eq!(run(&state, "r1", Some(id), own).await, Some(0));
    assert_eq!(read(&state, id).await.settings, None, "nothing was written");
    assert_eq!(run(&state, "r2", Some(id), json!({})).await, None);
}

/// A run saved to no conversation applies what it says, with nowhere to
/// remember it.
#[tokio::test]
async fn a_run_with_no_conversation_applies_what_it_says_and_remembers_nothing() {
    let (_dir, state) = state().await;
    let said = |word: &str| json!({ "thinking": word, "reasoning_budget_tokens": 4096 });
    assert_eq!(run(&state, "r1", None, said("off")).await, Some(0));
    assert_eq!(run(&state, "r2", None, said("default")).await, Some(4096));
    assert_eq!(run(&state, "r3", None, json!({})).await, None);
    let chats = state.core.chat_history().list_conversations().await;
    assert!(chats.unwrap().is_empty());
}

/// The door's refusal of `said` beside a run's body: its status, its code
/// and its message.
async fn refused(state: &AppState, id: i64, said: &Value) -> (u16, &'static str, String) {
    match create_run(state, "r1", body(Some(id), said)).await {
        Err(HttpError::Coded {
            status,
            code,
            message,
        }) => (status.as_u16(), code, message),
        other => panic!("not a coded refusal: {other:?}"),
    }
}

/// A word that is neither is not a run's body: it does not parse, and the
/// door refuses it as a body, before a conversation is read or a run is
/// made. Nothing serves this harness's port, so the door refuses a valid
/// body too, with the same status and code. Only the message tells the two
/// apart: a bad word is refused as a body, and `off` is read and then
/// refused for its port.
#[tokio::test]
async fn an_unknown_thinking_value_is_refused_400() {
    const NOT_A_BODY: &str = "an agent run's body is an agent chat request";
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let parsed = |said: &Value| serde_json::from_value::<AgentRunRequest>(body(Some(id), said));

    for word in [json!("on"), json!("OFF"), json!(0)] {
        let said = json!({ "thinking": word });
        assert!(parsed(&said).is_err(), "{said} is not a run's body");
        let (status, code, message) = refused(&state, id, &said).await;
        assert_eq!((status, code), (400, "invalid_request"));
        assert!(message.starts_with(NOT_A_BODY), "{said}: {message}");
    }
    for said in [
        json!({ "thinking": "off" }),
        json!({ "thinking": "default" }),
        json!({}),
    ] {
        assert!(parsed(&said).is_ok(), "{said} is a run's body");
    }
    let (status, code, message) = refused(&state, id, &json!({ "thinking": "off" })).await;
    assert_eq!((status, code), (400, "invalid_request"));
    assert!(!message.starts_with(NOT_A_BODY), "{message}");
    assert!(
        message.contains("port 9000"),
        "the port's refusal: {message}"
    );

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
    let id = conversation(&state).await;
    let here = ModelRef {
        machine: Machine::Local,
        id: model,
    };
    for (run_id, said) in [("r1", json!({ "thinking": "off" })), ("r2", json!({}))] {
        let request = serde_json::from_value(body(Some(id), &said)).unwrap();
        let (_, transcript) = plan(&state, request).await.unwrap();
        let (mut p, _) = prepared(finished_reply(), End::Finish);
        p.local_model = Some((19_555, model));
        let started = start_on(&state, run_id, transcript, p).await;
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

/// A run refused after its id is reserved, here for a row to replace that
/// is not the conversation's, has written nothing of thinking either.
#[tokio::test]
async fn a_run_refused_at_its_launch_remembers_nothing() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let transcript = Transcript {
        replace_from: Some(4242),
        remember: Some(Some(Thinking::Off)),
        ..saving(id)
    };
    let refused = start(&state, "r1", transcript).await;
    assert!(
        matches!(&refused, Err(HttpError::Coded { code, .. }) if *code == "message_not_found"),
        "{refused:?}"
    );
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
    assert!(start(&state, "r1", forget).await.unwrap());
    settled(&state).await;
    assert_eq!(read(&state, id).await.settings, None);
}
