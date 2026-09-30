//! A conversation has one live reply: a second run for it is a `409
//! conflict` naming the live run, and writes nothing.

use std::sync::Arc;

use axum::http::StatusCode;
use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::RunsPort as _;
use tokio::sync::Semaphore;

use super::launch::{Transcript, launch};
use super::run_fixture::{
    End, LOCAL, conversation, finished_reply, logged, prepared, reply, saved, settled, start, state,
};
use crate::error::HttpError;

/// A permit of a semaphore of its own: the daemon's one slot is the live
/// run's, and the refusal under test must come from the runs, not the slots.
fn spare_permit() -> tokio::sync::OwnedSemaphorePermit {
    Arc::new(Semaphore::new(1)).try_acquire_owned().unwrap()
}

#[tokio::test]
async fn a_second_run_for_a_live_conversation_is_a_conflict_and_writes_nothing() {
    let (_dir, state) = state().await;
    let id = conversation(&state).await;
    let (p, _) = prepared(reply(), End::Hang);
    start(&state, "e1", Some(id), p).await;
    logged(&state, "e1", reply().len()).await;
    let rows = saved(&state, id).await;
    assert_eq!(rows.len(), 1, "the first run's user message");

    for replace_from in [None, Some(rows[0].id)] {
        let (p, _) = prepared(finished_reply(), End::Finish);
        let transcript = Transcript {
            conversation_id: Some(id),
            replace_from,
        };
        let refused = launch(&state, "e2", LOCAL, transcript, p, spare_permit()).await;

        let Err(HttpError::Coded {
            status,
            code,
            message,
        }) = refused
        else {
            panic!("refused");
        };
        assert_eq!((status, code), (StatusCode::CONFLICT, "conflict"));
        assert!(message.contains("run e1"), "{message}");
        let after: Vec<i64> = saved(&state, id).await.iter().map(|r| r.id).collect();
        assert_eq!(after, [rows[0].id], "no user row written or replaced");
        assert!(state.runs.get(&LOCAL, "e2").is_err(), "no run left behind");
    }

    state.runs.cancel(&LOCAL, "e1").unwrap();
    settled(&state).await;
    assert_eq!(
        state.runs.get(&LOCAL, "e1").unwrap().status,
        RunStatus::Cancelled
    );
    let (p, _) = prepared(finished_reply(), End::Finish);
    start(&state, "e3", Some(id), p).await;
}
