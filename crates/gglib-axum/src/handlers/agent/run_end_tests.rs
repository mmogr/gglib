//! How an agent run ends: cancel, failure and shutdown drop the loop, free
//! its slot and save what arrived; a repeated create saves nothing twice.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use gglib_core::domain::agent::INCOMPLETE_KEY;
use gglib_core::domain::chat::MessageRole;
use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::RunsPort as _;
use serde_json::json;

use super::run::launch;
use super::run_fixture::{
    End, LOCAL, conversation, logged, meta, prepared, reply, saved, settled, start, state,
};

#[tokio::test]
async fn cancel_drops_the_loop_frees_the_slot_and_saves_what_arrived_marked() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, dropped) = prepared(reply(), End::Hang);
    start(&state, "a1", Some(id), p).await;
    logged(&state, "a1", reply().len()).await;
    assert_eq!(
        state.agent_semaphore.available_permits(),
        0,
        "the run holds it"
    );

    let info = state.runs.cancel(&LOCAL, "a1").unwrap();
    settled(&state).await;

    assert_eq!(info.status, RunStatus::Cancelled);
    assert_eq!(dropped.load(Ordering::SeqCst), 1, "the loop was dropped");
    assert_eq!(state.agent_semaphore.available_permits(), 1);
    let rows = saved(&state, id).await;
    assert_eq!(
        rows.len(),
        4,
        "the user, the turn, its result, the partial answer"
    );
    let last = rows.last().unwrap();
    assert_eq!(
        (last.role, last.content.as_str()),
        (MessageRole::Assistant, "ANSWER-SECRET")
    );
    assert_eq!(meta(last, INCOMPLETE_KEY), json!(true));
}

#[tokio::test]
async fn a_failed_run_ends_with_fixed_text_and_saves_its_reply_marked() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, _) = prepared(reply(), End::Fail);
    start(&state, "a1", Some(id), p).await;
    settled(&state).await;

    let info = state.runs.get(&LOCAL, "a1").unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    let error = info.error.unwrap();
    assert_eq!(error.code, "loop_detected");
    assert!(!error.message.contains("SIGNATURE-SECRET"));
    assert_eq!(state.agent_semaphore.available_permits(), 1);
    let rows = saved(&state, id).await;
    assert_eq!(meta(rows.last().unwrap(), INCOMPLETE_KEY), json!(true));
}

#[tokio::test]
async fn shutdown_drops_the_loop_and_the_reply_is_saved_before_drained() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, dropped) = prepared(reply()[..1].to_vec(), End::Hang);
    start(&state, "a1", Some(id), p).await;
    logged(&state, "a1", 1).await;

    state.runs.shutdown();
    settled(&state).await;

    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert_eq!(state.agent_semaphore.available_permits(), 1);
    let rows = saved(&state, id).await;
    assert_eq!(rows.len(), 2, "the user's message and the stopped reply");
    assert_eq!(meta(&rows[1], INCOMPLETE_KEY), json!(true));
}

#[tokio::test]
async fn a_repeated_create_saves_the_users_message_once() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (first, _) = prepared(Vec::new(), End::Hang);
    start(&state, "a1", Some(id), first).await;

    let (again, dropped) = prepared(Vec::new(), End::Hang);
    let spare = Arc::new(tokio::sync::Semaphore::new(1));
    let created = launch(
        &state,
        "a1",
        Some(id),
        again,
        spare.try_acquire_owned().unwrap(),
    )
    .await
    .unwrap();

    assert!(!created.created);
    assert_eq!(saved(&state, id).await.len(), 1);
    assert_eq!(
        dropped.load(Ordering::SeqCst),
        0,
        "the second loop never ran"
    );
}
