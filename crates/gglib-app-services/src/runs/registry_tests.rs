//! The registry's own rules: create, the log and its readers, status and
//! cancel. Limits, scope and retention have files of their own.

use std::sync::atomic::Ordering;

use gglib_core::domain::runs::{RunError, RunStatus};
use gglib_core::ports::{RunEvent, RunScope, RunsError, RunsPort};
use serde_json::json;

use super::test_executor::{Cmd, body, drain, next, registry, until};

const LOCAL: RunScope = RunScope::Local;

#[tokio::test]
async fn a_run_starts_queued_with_its_model_and_the_clocks_time() {
    let (runs, _, _) = registry();

    let created = runs.create(LOCAL, "r1", body("qwen")).expect("created");

    assert!(created.created);
    let info = created.info;
    assert_eq!(info.id, "r1");
    assert_eq!(info.status, RunStatus::Queued);
    assert_eq!(info.model.as_deref(), Some("qwen"));
    assert_eq!(info.device, None);
    assert_eq!(info.created_at_ms, 1_000);
    assert_eq!(info.finished_at_ms, None);
    assert_eq!(info.last_seq, 0);
}

#[tokio::test]
async fn a_repeat_of_an_id_in_the_same_scope_returns_that_run_and_starts_nothing() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).expect("created");
    script.send(Cmd::Frame("a".into())).unwrap();
    script.send(Cmd::Finish(Ok(()))).unwrap();
    drain(runs.events(&LOCAL, "r1", 0).unwrap()).await;

    let again = runs.create(LOCAL, "r1", body("other")).expect("answered");

    assert!(!again.created);
    assert_eq!(again.info.status, RunStatus::Completed);
    assert_eq!(again.info.model.as_deref(), Some("m"));
    tokio::task::yield_now().await;
    assert_eq!(executor.started.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_id_outside_the_charset_or_a_body_that_is_not_an_object_is_refused() {
    let (runs, executor, _) = registry();
    for id in ["", "a/b", "..", "a.b", &"x".repeat(65)] {
        assert_eq!(
            runs.create(LOCAL, id, body("m")).unwrap_err(),
            RunsError::InvalidId,
            "{id:?}"
        );
    }
    assert_eq!(
        runs.create(LOCAL, "r1", json!(["not", "an", "object"]))
            .unwrap_err(),
        RunsError::InvalidBody
    );
    assert_eq!(RunsError::InvalidId.code(), "invalid_request");
    assert_eq!(RunsError::InvalidId.http_status(), 400);
    assert!(runs.list(&LOCAL).runs.is_empty());
    assert_eq!(executor.started.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_reader_gets_every_event_after_its_cursor_then_live_ones_then_the_end() {
    let (runs, executor, clock) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    script.send(Cmd::Start).unwrap();
    for frame in ["f1", "f2", "f3"] {
        script.send(Cmd::Frame(frame.into())).unwrap();
    }

    let mut events = runs.events(&LOCAL, "r1", 1).unwrap();
    assert_eq!(
        next(&mut events).await,
        Some(RunEvent::Frame {
            seq: 2,
            data: "f2".into()
        })
    );
    assert_eq!(
        next(&mut events).await,
        Some(RunEvent::Frame {
            seq: 3,
            data: "f3".into()
        })
    );
    script.send(Cmd::Frame("f4".into())).unwrap();
    assert_eq!(
        next(&mut events).await,
        Some(RunEvent::Frame {
            seq: 4,
            data: "f4".into()
        })
    );
    clock.advance(500);
    script.send(Cmd::Finish(Ok(()))).unwrap();
    let Some(RunEvent::End(info)) = next(&mut events).await else {
        panic!("the run's end follows its last event");
    };
    assert_eq!(info.status, RunStatus::Completed);
    assert_eq!(info.last_seq, 4);
    assert_eq!(info.finished_at_ms, Some(1_500));
    assert_eq!(next(&mut events).await, None, "nothing follows the end");

    let (frames, end) = drain(runs.events(&LOCAL, "r1", 0).unwrap()).await;
    let seqs: Vec<u32> = frames.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(seqs, [1, 2, 3, 4]);
    assert_eq!(frames[0].1, "f1");
    assert_eq!(end.map(|i| i.status), Some(RunStatus::Completed));
}

#[tokio::test]
async fn a_reader_that_falls_far_behind_still_gets_every_event_in_order() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    let events = runs.events(&LOCAL, "r1", 0).unwrap();
    for n in 1..=5_000 {
        script.send(Cmd::Frame(format!("frame {n}"))).unwrap();
    }
    script.send(Cmd::Finish(Ok(()))).unwrap();

    let (frames, end) = drain(events).await;

    assert_eq!(frames.len(), 5_000);
    for (i, (seq, data)) in frames.iter().enumerate() {
        assert_eq!(*seq as usize, i + 1);
        assert_eq!(*data, format!("frame {}", i + 1));
    }
    assert_eq!(end.map(|i| i.last_seq), Some(5_000));
}

#[tokio::test]
async fn a_reader_leaving_changes_nothing() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    script.send(Cmd::Frame("f1".into())).unwrap();
    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();
    assert!(next(&mut events).await.is_some());
    drop(events);

    script.send(Cmd::Frame("f2".into())).unwrap();
    script.send(Cmd::Finish(Ok(()))).unwrap();
    let (frames, end) = drain(runs.events(&LOCAL, "r1", 0).unwrap()).await;

    assert_eq!(frames.len(), 2);
    assert_eq!(end.map(|i| i.status), Some(RunStatus::Completed));
    assert_eq!(
        executor.dropped.load(Ordering::SeqCst),
        1,
        "it ran to its end"
    );
}

#[tokio::test]
async fn a_run_is_in_progress_once_the_upstream_answers_and_failed_with_its_error() {
    let (runs, executor, clock) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    script.send(Cmd::Start).unwrap();
    script.send(Cmd::Frame("f1".into())).unwrap();
    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();
    next(&mut events).await;

    let info = runs.get(&LOCAL, "r1").unwrap();
    assert_eq!(info.status, RunStatus::InProgress);
    assert_eq!(info.last_seq, 1);
    assert_eq!(info.finished_at_ms, None);

    clock.advance(42);
    let error = RunError {
        code: "upstream_error".into(),
        message: "The reply ended early.".into(),
    };
    script.send(Cmd::Finish(Err(error.clone()))).unwrap();
    let Some(RunEvent::End(info)) = next(&mut events).await else {
        panic!("the run ends");
    };
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.error, Some(error));
    assert_eq!(info.finished_at_ms, Some(1_042));
}

#[tokio::test]
async fn cancel_ends_a_run_cancelled_drops_its_upstream_and_is_idempotent() {
    let (runs, executor, clock) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    script.send(Cmd::Start).unwrap();
    script.send(Cmd::Frame("f1".into())).unwrap();
    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();
    next(&mut events).await;
    clock.advance(7);

    let first = runs.cancel(&LOCAL, "r1").unwrap();
    clock.advance(7);
    let second = runs.cancel(&LOCAL, "r1").unwrap();

    assert_eq!(first.status, RunStatus::Cancelled);
    assert_eq!(first.finished_at_ms, Some(1_007));
    assert_eq!(second, first, "a second cancel changes nothing");
    let Some(RunEvent::End(info)) = next(&mut events).await else {
        panic!("a reader sees the cancel");
    };
    assert_eq!(info.status, RunStatus::Cancelled);
    until(|| executor.dropped.load(Ordering::SeqCst) == 1).await;
    assert_eq!(runs.get(&LOCAL, "r1").unwrap().status, RunStatus::Cancelled);
}

#[tokio::test]
async fn cancelling_an_ended_run_leaves_it_as_it_ended() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    script.send(Cmd::Finish(Ok(()))).unwrap();
    drain(runs.events(&LOCAL, "r1", 0).unwrap()).await;

    assert_eq!(
        runs.cancel(&LOCAL, "r1").unwrap().status,
        RunStatus::Completed
    );
}

#[tokio::test]
async fn the_list_is_newest_first() {
    let (runs, _, clock) = registry();
    for id in ["a", "b", "c"] {
        runs.create(LOCAL, id, body("m")).unwrap();
        clock.advance(1);
    }
    let ids: Vec<String> = runs.list(&LOCAL).runs.into_iter().map(|r| r.id).collect();
    assert_eq!(ids, ["c", "b", "a"]);
}
