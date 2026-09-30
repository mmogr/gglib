//! A run has one ending, shown to everyone at once: while its end is being
//! handled every path shows it `in_progress`, and once handled its status
//! never changes. Retention waits for the handling too.

use gglib_core::domain::runs::{RunError, RunInfo, RunKind, RunStatus};
use gglib_core::ports::{RunScope, RunsPort};
use tokio::sync::oneshot;

use super::cell::KEEP_AFTER_END_MS;
use super::local::{Reservation, RunEnded};
use super::test_executor::{HandClock, drain, registry};
use super::{RunRegistry, RunSpec};

const LOCAL: RunScope = RunScope::Local;

fn spec() -> RunSpec {
    RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: None,
    }
}

/// Start `a1`, whose work completes at once and whose end handler signals
/// `entered`, then waits for `release` and answers with `outcome`.
fn held(
    runs: &RunRegistry,
    outcome: Result<(), RunError>,
) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
    let (entered_tx, entered) = oneshot::channel();
    let (release, released) = oneshot::channel::<()>();
    let ended: RunEnded = Box::new(move |_, _| {
        Box::pin(async move {
            let _ = entered_tx.send(());
            let _ = released.await;
            outcome
        })
    });
    let Ok(Reservation::New(reserved)) = runs.reserve(RunScope::Local, "a1", spec()) else {
        panic!("a new reservation");
    };
    reserved.start(|_| Box::pin(async { Ok(()) }), ended);
    (entered, release)
}

/// The run as get, the list, cancel's answer, a repeated create and the
/// id lookup each show it.
fn every_path(runs: &RunRegistry) -> Vec<RunInfo> {
    let repeat = match runs.reserve(RunScope::Local, "a1", spec()).unwrap() {
        Reservation::Existing(info) => info,
        Reservation::New(_) => panic!("the run has the id"),
    };
    vec![
        runs.get(&LOCAL, "a1").unwrap(),
        runs.list(&LOCAL).runs.remove(0),
        runs.cancel(&LOCAL, "a1").unwrap(),
        repeat,
        runs.existing(&RunScope::Local, "a1").unwrap().unwrap(),
    ]
}

fn still_going(info: &RunInfo) -> bool {
    info.status == RunStatus::InProgress && info.finished_at_ms.is_none() && info.error.is_none()
}

async fn one_ending(outcome: Result<(), RunError>, want: RunStatus) -> RunInfo {
    let (runs, _, _) = registry();
    let (entered, release) = held(&runs, outcome);
    entered.await.unwrap();

    let mut seen: Vec<RunStatus> = Vec::new();
    for _ in 0..3 {
        for info in every_path(&runs) {
            assert!(still_going(&info), "not ended while held: {info:?}");
            seen.push(info.status);
        }
    }
    release.send(()).unwrap();
    let (_, end) = drain(runs.events(&LOCAL, "a1", 0).unwrap()).await;
    let end = end.expect("the end");
    assert_eq!(end.status, want);
    for _ in 0..3 {
        for info in every_path(&runs) {
            assert_eq!(info.status, want, "{info:?}");
            assert!(info.finished_at_ms.is_some());
            seen.push(info.status);
        }
    }
    let mut terminal: Vec<RunStatus> = Vec::new();
    for status in seen.into_iter().filter(|s| s.is_terminal()) {
        if !terminal.contains(&status) {
            terminal.push(status);
        }
    }
    assert_eq!(terminal, [want], "one ending, never two");
    end
}

#[tokio::test]
async fn while_the_end_is_handled_every_path_says_in_progress_then_completed() {
    let end = one_ending(Ok(()), RunStatus::Completed).await;
    assert_eq!(end.error, None);
}

#[tokio::test]
async fn a_failed_handling_is_the_only_ending_anyone_sees() {
    let error = RunError {
        code: "transcript_not_saved".to_owned(),
        message: "fixed".to_owned(),
    };
    let end = one_ending(Err(error.clone()), RunStatus::Failed).await;
    assert_eq!(end.error, Some(error));
}

#[tokio::test]
async fn a_panic_while_handling_the_end_fails_the_run() {
    let (runs, _, _) = registry();
    let ended: RunEnded = Box::new(|_, _| Box::pin(async { panic!("the save panicked") }));
    let Ok(Reservation::New(reserved)) = runs.reserve(RunScope::Local, "a1", spec()) else {
        panic!("a new reservation");
    };
    reserved.start(|_| Box::pin(async { Ok(()) }), ended);

    let (_, end) = drain(runs.events(&LOCAL, "a1", 0).unwrap()).await;

    let end = end.expect("the end");
    assert_eq!(end.status, RunStatus::Failed);
    assert_eq!(end.error.map(|e| e.code).as_deref(), Some("run_panicked"));
    assert_eq!(runs.get(&LOCAL, "a1").unwrap().status, RunStatus::Failed);
}

#[tokio::test]
async fn retention_ignores_a_run_whose_end_is_not_yet_handled() {
    let clock = HandClock::at(1_000);
    let runs = RunRegistry::new(registry().1, clock.clock());
    let (entered, release) = held(&runs, Ok(()));
    entered.await.unwrap();

    clock.advance(KEEP_AFTER_END_MS + 1);
    assert_eq!(runs.list(&LOCAL).runs.len(), 1, "kept while held");

    release.send(()).unwrap();
    drain(runs.events(&LOCAL, "a1", 0).unwrap()).await;
    clock.advance(KEEP_AFTER_END_MS + 1);
    assert!(runs.list(&LOCAL).runs.is_empty(), "dropped once handled");
}
