//! The limits: 32 runs, 8 MB of log each, and shutdown dropping them all.

use std::sync::atomic::Ordering;

use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::{RunScope, RunsError, RunsPort};

use super::registry::MAX_RUNS;
use super::test_executor::{Cmd, body, drain, next, registry, until};

const LOCAL: RunScope = RunScope::Local;

#[tokio::test]
async fn a_log_past_8_mb_fails_the_run_with_log_full_and_drops_its_upstream() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    let mib = "x".repeat(1024 * 1024);
    for _ in 0..9 {
        script.send(Cmd::Frame(mib.clone())).unwrap();
    }

    let (frames, end) = drain(runs.events(&LOCAL, "r1", 0).unwrap()).await;

    assert_eq!(frames.len(), 8, "8 MB exactly still fits");
    let info = end.expect("the run ends");
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.error.map(|e| e.code).as_deref(), Some("log_full"));
    assert_eq!(info.last_seq, 8);
    until(|| executor.dropped.load(Ordering::SeqCst) == 1).await;
}

#[tokio::test]
async fn with_every_slot_live_a_new_run_is_refused() {
    let (runs, _, _) = registry();
    for n in 0..MAX_RUNS {
        runs.create(LOCAL, &format!("r{n}"), body("m")).unwrap();
    }

    let refused = runs.create(LOCAL, "one-more", body("m")).unwrap_err();

    assert_eq!(refused, RunsError::TooManyRuns);
    assert_eq!(
        (refused.code(), refused.http_status()),
        ("too_many_runs", 429)
    );
    assert_eq!(runs.list(&LOCAL).runs.len(), 32);
}

#[tokio::test]
async fn at_the_limit_the_oldest_ended_run_makes_room() {
    let (runs, executor, _) = registry();
    let fifth = executor.script("r5");
    let tenth = executor.script("r10");
    for n in 0..MAX_RUNS {
        runs.create(LOCAL, &format!("r{n}"), body("m")).unwrap();
    }
    // The younger run ends first, so "oldest" is by creation, not by ending.
    tenth.send(Cmd::Finish(Ok(()))).unwrap();
    drain(runs.events(&LOCAL, "r10", 0).unwrap()).await;
    fifth.send(Cmd::Finish(Ok(()))).unwrap();
    drain(runs.events(&LOCAL, "r5", 0).unwrap()).await;

    runs.create(LOCAL, "one-more", body("m"))
        .expect("room was made");

    assert_eq!(runs.get(&LOCAL, "r5").unwrap_err(), RunsError::NotFound);
    assert!(runs.get(&LOCAL, "r10").is_ok());
    assert!(
        runs.get(&LOCAL, "r0").is_ok(),
        "a live run is never the one dropped"
    );
    assert_eq!(runs.list(&LOCAL).runs.len(), 32);
}

#[tokio::test]
async fn shutdown_cancels_and_drops_every_run_and_ends_its_readers() {
    let (runs, executor, _) = registry();
    let script = executor.script("r1");
    runs.create(LOCAL, "r1", body("m")).unwrap();
    runs.create(RunScope::Device("phone".into()), "r2", body("m"))
        .unwrap();
    script.send(Cmd::Frame("f1".into())).unwrap();
    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();
    next(&mut events).await;

    runs.shutdown();

    assert_eq!(
        next(&mut events).await,
        None,
        "the reader ends, with no end"
    );
    assert!(runs.list(&LOCAL).runs.is_empty());
    until(|| executor.dropped.load(Ordering::SeqCst) == 2).await;
}
