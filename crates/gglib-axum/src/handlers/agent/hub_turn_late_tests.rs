//! A device's turn exists before its model loads: the `PUT` answers at once,
//! the run says it is waiting, and what is refused after the run exists ends
//! it `failed` with the code the `PUT` used to answer, with no row written.

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use gglib_core::domain::chat::MessageRole;
use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::{RunEvent, RunsPort as _};
use tokio::sync::Notify;

use super::hub_turn_tests::{chat, device, failed_with, turn};
use super::{begin, plan, start};
use crate::handlers::agent::compose::take_permit;
use crate::handlers::agent::launch::LatePrepare;
use crate::handlers::agent::run::coded;
use crate::handlers::agent::run_fixture::{
    End, drain, finished_reply, logged, prepared, saved, settled, state,
};
use crate::state::AppState;

/// The `type` of each frame `device`'s run `id` logged.
async fn frames(state: &AppState, name: &str, id: &str) -> Vec<String> {
    let mut events = state.runs.events(&device(name), id, 0).unwrap();
    let mut kinds = Vec::new();
    while let Ok(Some(event)) = tokio::time::timeout(
        Duration::from_secs(2),
        futures_util::StreamExt::next(&mut events),
    )
    .await
    {
        if let RunEvent::Frame { data, .. } = event {
            let frame: serde_json::Value = serde_json::from_str(&data).unwrap();
            kinds.push(frame["type"].as_str().unwrap().to_owned());
        }
    }
    kinds
}

/// A model that cannot be loaded no longer refuses the `PUT`: the run is
/// made, says it waits for the load, and ends `failed` with
/// `model_unavailable`. Nothing was written, the chat is free again, and so
/// is the slot.
#[tokio::test]
async fn a_load_that_fails_ends_the_run_failed_and_writes_nothing() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;

    let created = start(&state, "phone", "d1", turn(id, "second"))
        .await
        .unwrap();

    assert!(created.created);
    assert_eq!(created.info.conversation_id, Some(id));
    assert_eq!(
        failed_with(&state, "phone", "d1").await,
        "model_unavailable"
    );
    let run = state
        .runs
        .existing(&device("phone"), "d1")
        .unwrap()
        .unwrap();
    let error = run.error.unwrap();
    assert_eq!(
        error.message,
        "the chat's model could not be loaded on the hub"
    );
    assert_eq!(frames(&state, "phone", "d1").await, ["waiting"]);
    assert_eq!(saved(&state, id).await.len(), 2, "no row was written");
    assert_eq!(state.agent_semaphore.available_permits(), 1);
    assert!(state.runs.live_on(id).is_none(), "the chat is free again");
}

/// A late step that waits for its model, as a cold load does, until
/// `loaded` is notified, then hands back a scripted loop.
fn slow(
    loaded: &Arc<Notify>,
    messages: Vec<gglib_core::domain::agent::AgentMessage>,
) -> LatePrepare {
    let loaded = Arc::clone(loaded);
    Box::new(move |loading| {
        Box::pin(async move {
            loading.waiting();
            loaded.notified().await;
            let (mut p, _) = prepared(finished_reply(), End::Finish);
            p.messages = messages;
            Ok(p)
        })
    })
}

/// While the model loads, the `PUT` has already answered: the run exists,
/// has logged that it waits, holds the chat, and has written no row; a
/// retry of the same id finds it. Once loaded, the message and the reply
/// are saved as before.
#[tokio::test]
async fn the_put_answers_during_a_cold_load_and_the_run_waits_then_replies() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let plan = plan(&state, turn(id, "second")).await.unwrap();
    let loaded = Arc::new(Notify::new());
    let late = slow(&loaded, plan.chat.messages);

    let created = begin(
        &state,
        "phone",
        "d1",
        "qwen".to_owned(),
        plan.transcript,
        late,
        take_permit(&state).unwrap(),
    )
    .unwrap();

    assert!(created.created);
    assert_eq!(created.info.model.as_deref(), Some("qwen"));
    logged(&state, "d1", 1).await;
    assert_eq!(
        saved(&state, id).await.len(),
        2,
        "nothing written while it loads"
    );
    assert_eq!(state.runs.live_on(id).as_deref(), Some("d1"));
    let again = start(&state, "phone", "d1", turn(id, "second"))
        .await
        .unwrap();
    assert!(!again.created, "a retry finds the run");
    assert!(!again.info.status.is_terminal());

    loaded.notify_one();
    settled(&state).await;

    let run = state
        .runs
        .existing(&device("phone"), "d1")
        .unwrap()
        .unwrap();
    assert_eq!(run.status, RunStatus::Completed);
    let kinds = frames(&state, "phone", "d1").await;
    assert_eq!(kinds.first().map(String::as_str), Some("waiting"));
    assert!(kinds.len() > 1, "the reply follows: {kinds:?}");
    let rows = saved(&state, id).await;
    let said: Vec<(MessageRole, &str)> =
        rows.iter().map(|r| (r.role, r.content.as_str())).collect();
    assert_eq!(said[2], (MessageRole::User, "second"));
    assert_eq!(
        said.last().copied(),
        Some((MessageRole::Assistant, "ANSWER-SECRET"))
    );
}

/// Whatever is refused once the run exists ends it with that refusal's code
/// and words, and neither the message nor a reply is saved.
#[tokio::test]
async fn a_refusal_after_the_run_exists_is_its_error_and_writes_no_row() {
    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let plan = plan(&state, turn(id, "second")).await.unwrap();
    let late: LatePrepare = Box::new(|_| {
        Box::pin(async {
            Err(coded(
                StatusCode::SERVICE_UNAVAILABLE,
                "unavailable",
                "no server is running on that port",
            ))
        })
    });

    begin(
        &state,
        "phone",
        "d1",
        "qwen".to_owned(),
        plan.transcript,
        late,
        take_permit(&state).unwrap(),
    )
    .unwrap();

    assert_eq!(failed_with(&state, "phone", "d1").await, "unavailable");
    let run = state
        .runs
        .existing(&device("phone"), "d1")
        .unwrap()
        .unwrap();
    assert_eq!(
        run.error.unwrap().message,
        "no server is running on that port"
    );
    assert_eq!(saved(&state, id).await.len(), 2, "no row was written");
    let (events, end) = drain(state.runs.events(&device("phone"), "d1", 0).unwrap()).await;
    assert_eq!(events, 0);
    assert_eq!(end.map(|info| info.status), Some(RunStatus::Failed));
}

/// A refusal of the first write is the run's error too, and its end saves
/// no reply: a turn that would run on another machine than its chat's.
#[tokio::test]
async fn a_refused_first_write_saves_no_reply() {
    use gglib_core::domain::chat::ConversationSettings;
    use gglib_core::domain::{Machine, ModelRef};

    use crate::handlers::agent::launch::ready;
    use crate::handlers::agent::run_fixture::saving;

    let (_dir, state) = state().await;
    let model = crate::handlers::agent::turn_fixture::model(&state, |_| {}).await;
    let here = ConversationSettings {
        model: Some(ModelRef {
            machine: Machine::Local,
            id: model,
        }),
        ..ConversationSettings::default()
    };
    let id = chat(&state, Some(here)).await;
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.far_model = Some(ModelRef {
        machine: Machine::Paired {
            fingerprint: "0a1b2c3d4e5f".to_owned(),
        },
        id: 7,
    });

    begin(
        &state,
        "phone",
        "d1",
        "qwen".to_owned(),
        saving(id),
        ready(p),
        take_permit(&state).unwrap(),
    )
    .unwrap();

    assert_eq!(failed_with(&state, "phone", "d1").await, "conflict");
    assert_eq!(saved(&state, id).await.len(), 2, "no row was written");
}

/// A turn whose loop was prepared and whose model is then gone from its
/// port, stopped or swapped in between, is refused at the hold: the run
/// ends `failed` with that refusal, and neither the message nor a reply
/// was written, because the rows are written only once the model is held.
#[tokio::test]
async fn a_refusal_at_the_model_hold_ends_the_run_failed_and_writes_no_row() {
    use crate::handlers::agent::launch::ready;

    let (_dir, state) = state().await;
    let id = chat(&state, None).await;
    let plan = plan(&state, turn(id, "second")).await.unwrap();
    let (mut p, _) = prepared(finished_reply(), End::Finish);
    p.messages = plan.chat.messages;
    // A local model on a port where nothing runs: the runtime holds none.
    p.local_model = Some((19_555, 7));

    begin(
        &state,
        "phone",
        "d1",
        "qwen".to_owned(),
        plan.transcript,
        ready(p),
        take_permit(&state).unwrap(),
    )
    .unwrap();

    assert_eq!(failed_with(&state, "phone", "d1").await, "unavailable");
    let run = state
        .runs
        .existing(&device("phone"), "d1")
        .unwrap()
        .unwrap();
    let message = run.error.unwrap().message;
    assert!(message.contains("stopped or swapped"), "{message}");
    assert_eq!(saved(&state, id).await.len(), 2, "no row was written");
    let (events, end) = drain(state.runs.events(&device("phone"), "d1", 0).unwrap()).await;
    assert_eq!(events, 0, "the loop never ran");
    assert_eq!(end.map(|info| info.status), Some(RunStatus::Failed));
}
