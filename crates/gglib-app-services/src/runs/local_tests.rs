//! A run this machine starts with prepared work: its kind and conversation,
//! the reservation, the end handed over whatever the end, and that the
//! port cannot make one.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gglib_core::domain::runs::{RunError, RunInfo, RunKind, RunStatus};
use gglib_core::ports::{RunScope, RunsError, RunsPort};
use serde_json::json;
use tokio::sync::oneshot;

use super::local::{Reservation, RunEnded, RunWork};
use super::test_executor::{drain, registry, until};
use super::{RunLog, RunRegistry, RunSpec};

fn agent(conversation_id: Option<i64>) -> RunSpec {
    RunSpec {
        kind: RunKind::Agent,
        model: Some("qwen".to_owned()),
        conversation_id,
    }
}

/// What the end hook was handed, once it is.
type Seen = Arc<Mutex<Option<(RunInfo, Vec<String>)>>>;

fn ended(seen: &Seen) -> RunEnded {
    let seen = Arc::clone(seen);
    Box::new(move |info, frames| {
        Box::pin(async move {
            let frames = frames.iter().map(ToString::to_string).collect();
            *seen.lock().unwrap() = Some((info, frames));
            Ok(())
        })
    })
}

/// Counts drops of the future it lives in.
struct Dropped(Arc<AtomicUsize>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Work that logs `frames`, then waits for `finish` and ends with it.
fn work(
    frames: &'static [&'static str],
    finish: oneshot::Receiver<Result<(), RunError>>,
    dropped: &Arc<AtomicUsize>,
) -> impl FnOnce(RunLog) -> RunWork {
    let guard = Dropped(Arc::clone(dropped));
    move |log| {
        Box::pin(async move {
            let _guard = guard;
            log.started();
            for frame in frames {
                let _ = log.append((*frame).to_owned());
            }
            match finish.await {
                Ok(outcome) => outcome,
                Err(_) => std::future::pending().await,
            }
        })
    }
}

fn start(
    runs: &RunRegistry,
    id: &str,
    frames: &'static [&'static str],
) -> (
    Seen,
    oneshot::Sender<Result<(), RunError>>,
    Arc<AtomicUsize>,
) {
    let seen = Seen::default();
    let dropped = Arc::new(AtomicUsize::new(0));
    let (finish, rx) = oneshot::channel();
    let Ok(Reservation::New(reserved)) = runs.reserve(RunScope::Local, id, agent(Some(7))) else {
        panic!("a new reservation");
    };
    reserved.start(work(frames, rx, &dropped), ended(&seen));
    (seen, finish, dropped)
}

async fn seen_end(seen: &Seen) -> (RunInfo, Vec<String>) {
    until(|| seen.lock().unwrap().is_some()).await;
    seen.lock().unwrap().clone().unwrap()
}

#[tokio::test]
async fn an_agent_run_carries_its_kind_and_conversation_and_hands_over_its_end() {
    let (runs, _, _) = registry();
    let (seen, finish, _) = start(&runs, "a1", &["one", "two"]);

    let info = runs.get(&RunScope::Local, "a1").unwrap();
    assert_eq!(info.kind, RunKind::Agent);
    assert_eq!(info.conversation_id, Some(7));
    finish.send(Ok(())).unwrap();

    let (end, frames) = seen_end(&seen).await;
    assert_eq!(end.status, RunStatus::Completed);
    assert_eq!(frames, ["one", "two"]);
    let (logged, last) = drain(runs.events(&RunScope::Local, "a1", 0).unwrap()).await;
    assert_eq!(logged.len(), 2);
    assert_eq!(last.map(|i| i.kind), Some(RunKind::Agent));
}

#[tokio::test]
async fn a_failed_run_hands_over_its_end_with_the_error() {
    let (runs, _, _) = registry();
    let (seen, finish, _) = start(&runs, "a1", &["one"]);
    let error = RunError {
        code: "loop_detected".to_owned(),
        message: "fixed".to_owned(),
    };

    finish.send(Err(error.clone())).unwrap();

    let (end, frames) = seen_end(&seen).await;
    assert_eq!(end.status, RunStatus::Failed);
    assert_eq!(end.error, Some(error));
    assert_eq!(frames, ["one"]);
}

#[tokio::test]
async fn cancel_drops_the_work_and_still_hands_over_the_end() {
    let (runs, _, _) = registry();
    let (seen, _finish, dropped) = start(&runs, "a1", &["one"]);
    until(|| runs.get(&RunScope::Local, "a1").unwrap().last_seq == 1).await;

    runs.cancel(&RunScope::Local, "a1").unwrap();

    let (end, frames) = seen_end(&seen).await;
    assert_eq!(end.status, RunStatus::Cancelled);
    assert_eq!(frames, ["one"]);
    assert_eq!(dropped.load(Ordering::SeqCst), 1, "the work was dropped");
}

#[tokio::test]
async fn shutdown_drops_the_work_hands_over_the_end_and_then_drains() {
    let (runs, _, _) = registry();
    let (seen, _finish, dropped) = start(&runs, "a1", &[]);

    runs.shutdown();
    tokio::time::timeout(Duration::from_secs(1), runs.drained())
        .await
        .expect("every run's end is handled");

    let (end, _) = seen
        .lock()
        .unwrap()
        .clone()
        .expect("the end was handed over");
    assert_eq!(end.status, RunStatus::Cancelled);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_repeated_id_answers_with_the_run_and_reserves_nothing() {
    let (runs, _, _) = registry();
    let (_, _finish, _) = start(&runs, "a1", &[]);

    let again = runs.reserve(RunScope::Local, "a1", agent(Some(9))).unwrap();

    let Reservation::Existing(info) = again else {
        panic!("the existing run");
    };
    assert_eq!(info.conversation_id, Some(7));
    assert_eq!(
        runs.existing(&RunScope::Local, "a1").unwrap().map(|i| i.id),
        Some("a1".into())
    );
}

#[tokio::test]
async fn a_reservation_dropped_unstarted_leaves_no_run() {
    let (runs, _, _) = registry();

    let reserved = runs.reserve(RunScope::Local, "a1", agent(None)).unwrap();
    assert!(runs.existing(&RunScope::Local, "a1").unwrap().is_some());
    drop(reserved);

    assert_eq!(runs.existing(&RunScope::Local, "a1").unwrap(), None);
    assert!(runs.list(&RunScope::Local).runs.is_empty());
}

#[tokio::test]
async fn a_device_runs_id_is_taken_for_this_machine() {
    let (runs, _, _) = registry();
    let phone = RunScope::Device("phone".into());
    runs.create(phone, "d1", json!({ "model": "m" })).unwrap();

    assert_eq!(
        runs.existing(&RunScope::Local, "d1"),
        Err(RunsError::IdTaken)
    );
    assert!(matches!(
        runs.reserve(RunScope::Local, "d1", agent(None)),
        Err(RunsError::IdTaken)
    ));
}

/// The proxy's door holds only the port, and the port's create makes a chat
/// run whatever the body says.
#[tokio::test]
async fn the_port_cannot_create_an_agent_run() {
    let (runs, _, _) = registry();
    let port: &dyn RunsPort = &runs;
    let body = json!({ "kind": "agent", "conversation_id": 7, "model": "m", "messages": [] });

    let created = port
        .create(RunScope::Device("phone".into()), "d1", body)
        .unwrap();

    assert_eq!(created.info.kind, RunKind::Chat);
    assert_eq!(created.info.conversation_id, None);
}
