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
    End, LOCAL, conversation, drain, finished_reply, logged, meta, prepared, reply, saved, settled,
    start, state,
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

/// A create dropped as soon as it started (the client gave up) still
/// finishes starting its run in its own task, so the retry finds that run
/// and the user's message is saved once.
#[tokio::test]
async fn a_create_dropped_part_way_and_retried_saves_the_users_message_once() {
    use std::task::{Context, Waker};

    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (first, _) = prepared(finished_reply(), End::Finish);
    let mut dropped = Box::pin(launch(
        &state,
        "a1",
        Some(id),
        first,
        super::compose::take_permit(&state).unwrap(),
    ));
    assert!(
        dropped
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(dropped);

    let (again, _) = prepared(Vec::new(), End::Hang);
    let spare = Arc::new(tokio::sync::Semaphore::new(1));
    let retried = launch(
        &state,
        "a1",
        Some(id),
        again,
        spare.try_acquire_owned().unwrap(),
    )
    .await
    .unwrap();

    assert!(!retried.created, "the retry found the run");
    let (_, end) = drain(state.runs.events(&LOCAL, "a1", 0).unwrap()).await;
    assert_eq!(end.map(|e| e.status), Some(RunStatus::Completed));
    let rows = saved(&state, id).await;
    let users = rows.iter().filter(|r| r.role == MessageRole::User).count();
    assert_eq!(users, 1);
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

/// A loop that panics mid-reply: the run ends failed, frees its slot, and
/// saves what arrived, marked.
#[tokio::test]
async fn a_panicking_loop_ends_the_run_failed_and_saves_what_arrived() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, _) = prepared(reply(), End::Panic);
    start(&state, "a1", Some(id), p).await;
    settled(&state).await;

    let info = state.runs.get(&LOCAL, "a1").unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.error.map(|e| e.code).as_deref(), Some("run_panicked"));
    assert_eq!(state.agent_semaphore.available_permits(), 1);
    let rows = saved(&state, id).await;
    assert_eq!(rows.first().map(|r| r.role), Some(MessageRole::User));
    assert_eq!(meta(rows.last().unwrap(), INCOMPLETE_KEY), json!(true));
}

/// A reply that cannot be saved (its conversation was deleted mid-run)
/// fails the run, visibly, and its events can still be read.
#[tokio::test]
async fn a_reply_that_cannot_be_saved_fails_the_run() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, _) = prepared(reply(), End::Hang);
    start(&state, "a1", Some(id), p).await;
    logged(&state, "a1", reply().len()).await;
    state
        .core
        .chat_history()
        .delete_conversation(id)
        .await
        .unwrap();

    state.runs.cancel(&LOCAL, "a1").unwrap();
    settled(&state).await;

    let info = state.runs.get(&LOCAL, "a1").unwrap();
    assert_eq!(info.status, RunStatus::Failed);
    let error = info.error.unwrap();
    assert_eq!(error.code, "transcript_not_saved");
    let (frames, end) = drain(state.runs.events(&LOCAL, "a1", 0).unwrap()).await;
    assert_eq!(frames, reply().len());
    assert_eq!(end.map(|e| e.status), Some(RunStatus::Failed));
}

/// A reply one of whose rows the database refuses is not saved at all: no
/// assistant row is left without the rows that follow it.
#[tokio::test]
async fn a_reply_with_a_refused_row_saves_none_of_it() {
    let (dir, state) = state().await;
    let id = conversation(&state).await;
    let url = format!("sqlite:{}", dir.path().join("gglib.db").display());
    let pool = sqlx::SqlitePool::connect(&url).await.unwrap();
    sqlx::query(
        "CREATE TRIGGER refuse_tool_rows BEFORE INSERT ON chat_messages \
         WHEN NEW.role = 'tool' BEGIN SELECT RAISE(ABORT, 'refused'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let (p, _) = prepared(finished_reply(), End::Finish);

    start(&state, "a1", Some(id), p).await;
    settled(&state).await;

    let rows = saved(&state, id).await;
    let roles: Vec<MessageRole> = rows.iter().map(|r| r.role).collect();
    assert_eq!(roles, [MessageRole::User], "only the user's message");
    let error = state.runs.get(&LOCAL, "a1").unwrap().error.unwrap();
    assert_eq!(error.code, "transcript_not_saved");
}
