//! Retention, on a clock the tests move: an ended run goes ten minutes after
//! its own scope read it to the end, or a day after it ended, whichever is
//! first. No test here sleeps for either.

use gglib_core::ports::{RunScope, RunsError, RunsPort};

use super::RunRegistry;
use super::test_executor::{Cmd, body, drain, next, registry};

const MINUTE: u64 = 60 * 1000;
const HOUR: u64 = 60 * MINUTE;

/// Wait for a run to end without reading it.
async fn ended(runs: &RunRegistry, id: &str) {
    while !runs.get(&RunScope::Local, id).unwrap().status.is_terminal() {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn an_ended_run_read_to_the_end_goes_ten_minutes_after_the_read() {
    let (runs, executor, clock) = registry();
    let script = executor.script("r1");
    runs.create(RunScope::Local, "r1", body("m")).unwrap();
    script.send(Cmd::Finish(Ok(()))).unwrap();
    ended(&runs, "r1").await;
    // Ended, but not yet read: an hour passes before anyone reads it.
    clock.advance(HOUR);
    drain(runs.events(&RunScope::Local, "r1", 0).unwrap()).await;

    clock.advance(10 * MINUTE - 1);
    assert!(runs.get(&RunScope::Local, "r1").is_ok());
    clock.advance(1);
    assert_eq!(
        runs.get(&RunScope::Local, "r1").unwrap_err(),
        RunsError::NotFound
    );
}

#[tokio::test]
async fn a_device_reading_its_own_run_to_the_end_starts_the_same_ten_minutes() {
    let (runs, executor, clock) = registry();
    let phone = RunScope::Device("phone".into());
    let script = executor.script("p1");
    runs.create(phone.clone(), "p1", body("m")).unwrap();
    script.send(Cmd::Finish(Ok(()))).unwrap();
    drain(runs.events(&phone, "p1", 0).unwrap()).await;

    clock.advance(10 * MINUTE);

    assert!(runs.list(&RunScope::Local).runs.is_empty());
}

#[tokio::test]
async fn an_ended_run_nobody_read_goes_a_day_after_it_ended() {
    let (runs, executor, clock) = registry();
    let script = executor.script("r1");
    runs.create(RunScope::Local, "r1", body("m")).unwrap();
    script.send(Cmd::Frame("f1".into())).unwrap();
    script.send(Cmd::Frame("f2".into())).unwrap();
    script.send(Cmd::Finish(Ok(()))).unwrap();
    // A reader that stops short of the end has not read it to the end.
    let mut events = runs.events(&RunScope::Local, "r1", 0).unwrap();
    next(&mut events).await;
    drop(events);
    ended(&runs, "r1").await;

    clock.advance(10 * MINUTE);
    assert!(runs.get(&RunScope::Local, "r1").is_ok());
    clock.advance(24 * HOUR - 10 * MINUTE - 1);
    assert!(runs.get(&RunScope::Local, "r1").is_ok());
    clock.advance(1);
    assert!(runs.list(&RunScope::Local).runs.is_empty());
}

#[tokio::test]
async fn a_live_run_is_never_dropped_by_retention() {
    let (runs, _, clock) = registry();
    runs.create(RunScope::Local, "r1", body("m")).unwrap();

    clock.advance(48 * HOUR);

    assert!(runs.get(&RunScope::Local, "r1").is_ok());
}
