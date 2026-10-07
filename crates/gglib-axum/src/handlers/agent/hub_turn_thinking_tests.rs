//! A device's turn and its chat's Thinking choice: what the turn runs with,
//! what the chat remembers once the run starts, and that a turn which
//! starts nothing writes nothing.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use gglib_core::domain::Thinking;
use gglib_core::domain::chat::{ConversationSettings, NewConversation};
use gglib_core::domain::hub_chats::HubTurn;

use super::hub_turn_tests::{chat, device, refused, turn};
use super::{begin, plan, start};
use crate::handlers::agent::compose::take_permit;
use crate::handlers::agent::launch::{Transcript, launch};
use crate::handlers::agent::run_fixture::{
    End, finished_reply, paced, prepared, saving, settled, state,
};
use crate::handlers::agent::run_thinking_tests::remembered;
use crate::state::AppState;

/// A turn on `conversation_id` that says `thinking`.
fn saying(conversation_id: i64, thinking: Thinking) -> HubTurn {
    HubTurn {
        thinking: Some(thinking),
        ..turn(conversation_id, "next")
    }
}

/// Read `turn` against its chat and run it to its end as the phone's run
/// `id`: the thinking budget it ran with.
async fn run(state: &AppState, id: &str, turn: HubTurn) -> Option<i32> {
    let plan = plan(state, turn).await.unwrap();
    let budget = plan.chat.reasoning_budget_tokens;
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.messages = plan.chat.messages;
    let free = take_permit(state);
    begin(state, "phone", id, plan.transcript, p, free.unwrap())
        .await
        .unwrap();
    settled(state).await;
    budget
}

/// A reply that never ends, as the phone's run `id`, saved as `transcript`
/// says. It holds the daemon's one agent slot.
async fn hang(state: &AppState, id: &str, transcript: Transcript) {
    let (p, _) = paced(finished_reply(), End::Hang, Duration::from_millis(1));
    let free = take_permit(state);
    launch(state, id, device("phone"), transcript, p, free.unwrap())
        .await
        .unwrap();
}

/// A permit of a semaphore of its own, for a launch while that slot is held.
fn spare_permit() -> OwnedSemaphorePermit {
    Arc::new(Semaphore::new(1)).try_acquire_owned().unwrap()
}

#[tokio::test]
async fn a_turn_that_says_off_runs_with_a_budget_of_zero_and_the_chat_remembers() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    assert_eq!(run(&state, "d1", saying(id, Thinking::Off)).await, Some(0));
    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));
}

/// A turn that says nothing has no budget on a chat that remembers nothing,
/// a budget of `0` on one that remembers `off`, and changes neither.
#[tokio::test]
async fn a_turn_that_says_nothing_runs_as_the_chat_remembers() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    assert_eq!(run(&state, "d1", turn(id, "one")).await, None);
    assert_eq!(remembered(&state, id).await, None);

    run(&state, "d2", saying(id, Thinking::Off)).await;
    assert_eq!(run(&state, "d3", turn(id, "three")).await, Some(0));
    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));
}

/// The forgetting turn and the one after it run the same: with no budget,
/// which is all a device's turn can have of its own.
#[tokio::test]
async fn a_turn_that_says_default_forgets_and_names_no_budget() {
    let (_dir, state) = state().await;
    let off = ConversationSettings {
        thinking: Some(Thinking::Off),
        ..ConversationSettings::default()
    };
    let id = chat(&state, Some(off)).await;
    let forgetting = run(&state, "d1", saying(id, Thinking::Default)).await;
    assert_eq!(forgetting, None);
    assert_eq!(remembered(&state, id).await, None);
    assert_eq!(run(&state, "d2", turn(id, "after")).await, None);
}

/// An empty turn, a turn while every agent slot is taken (refused after it
/// was read against the chat), and a turn on a chat with a live reply.
#[tokio::test]
async fn a_refused_turn_remembers_nothing() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let empty = HubTurn {
        thinking: Some(Thinking::Off),
        ..turn(id, " ")
    };
    let refusal = refused(start(&state, "phone", "d1", empty).await);
    assert_eq!(refusal, (400, "invalid_request"));

    let slot = take_permit(&state).expect("the one slot");
    let off = saying(id, Thinking::Off);
    let refusal = refused(start(&state, "phone", "d2", off.clone()).await);
    assert_eq!(refusal, (429, "agent_busy"));
    drop(slot);

    hang(&state, "d3", saving(id)).await;
    let refusal = refused(start(&state, "laptop", "d4", off).await);
    assert_eq!(refusal, (409, "conflict"));

    assert_eq!(remembered(&state, id).await, None);
    state.runs.shutdown();
}

/// The same id again is the run it already is, at the door and at the
/// launch a request racing its own retry reaches: what the repeat says of
/// thinking is not written.
#[tokio::test]
async fn a_repeated_run_id_writes_nothing_again() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let off = Transcript {
        remember: Some(Some(Thinking::Off)),
        ..saving(id)
    };
    hang(&state, "d1", off).await;
    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));

    let again = start(&state, "phone", "d1", saying(id, Thinking::Default)).await;
    assert!(!again.unwrap().created);
    let forget = Transcript {
        remember: Some(None),
        ..saving(id)
    };
    let (p, _) = prepared(finished_reply(), End::Finish);
    let again = launch(&state, "d1", device("phone"), forget, p, spare_permit()).await;
    assert!(!again.unwrap().created);

    assert_eq!(remembered(&state, id).await, Some(Thinking::Off));
    state.runs.shutdown();
}

/// Remembering changes that one field. The chat's limits, its tools, the
/// name of its model and the catalogue model it was made with all stay,
/// through `off` and back.
#[tokio::test]
async fn remembering_keeps_every_other_setting() {
    let (_dir, state) = state().await;
    let model = gglib_core::domain::NewModel::new(
        "qwen3-8b".to_owned(),
        std::path::PathBuf::from("/models/qwen3-8b.gguf"),
        8.0,
        chrono::Utc::now(),
    );
    let model = state.core.models().add(model).await.unwrap().id;
    let settings = ConversationSettings {
        model_name: Some("qwen3-8b".to_owned()),
        temperature: Some(0.5),
        tools: vec!["fs:read_file".to_owned()],
        max_iterations: Some(4),
        no_tools: Some(false),
        ..ConversationSettings::default()
    };
    let history = state.core.chat_history();
    let id = history
        .create_conversation(NewConversation {
            title: "t".to_owned(),
            model_id: Some(model),
            system_prompt: Some("Be brief.".to_owned()),
            settings: Some(settings.clone()),
        })
        .await
        .unwrap();

    run(&state, "d1", saying(id, Thinking::Off)).await;
    let after = history.get_conversation(id).await.unwrap().unwrap();
    let off = ConversationSettings {
        thinking: Some(Thinking::Off),
        ..settings.clone()
    };
    assert_eq!((after.model_id, after.settings), (Some(model), Some(off)));
    assert_eq!(after.system_prompt.as_deref(), Some("Be brief."));

    run(&state, "d2", saying(id, Thinking::Default)).await;
    let after = history.get_conversation(id).await.unwrap().unwrap();
    assert_eq!(
        (after.model_id, after.settings),
        (Some(model), Some(settings))
    );
}
