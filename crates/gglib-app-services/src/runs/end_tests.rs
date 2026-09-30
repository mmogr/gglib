//! A run's end is final when a reader is given it: a panic in the work
//! still ends the run, and handling the end (saving a reply) comes first
//! and can fail the run.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures_util::FutureExt as _;
use gglib_core::domain::runs::{RunError, RunKind, RunStatus};
use gglib_core::ports::{RunEvent, RunScope, RunsPort};
use tokio::sync::oneshot;

use super::local::{Reservation, RunEnded};
use super::test_executor::{Cmd, body, drain, registry};
use super::{RunRegistry, RunSpec};

const LOCAL: RunScope = RunScope::Local;

fn spec() -> RunSpec {
    RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: None,
    }
}

/// Counts drops of the future it lives in: the permit a run holds.
struct Held(Arc<AtomicUsize>);

impl Drop for Held {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// Start `id` with work that logs `frames` and panics, and a hook that
/// records how many frames it was handed.
fn panicking(runs: &RunRegistry, id: &str, frames: usize) -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let released = Arc::new(AtomicUsize::new(0));
    let handed = Arc::new(AtomicUsize::new(usize::MAX));
    let held = Held(Arc::clone(&released));
    let seen = Arc::clone(&handed);
    let ended: RunEnded = Box::new(move |_, logged| {
        Box::pin(async move {
            seen.store(logged.len(), Ordering::SeqCst);
            Ok(())
        })
    });
    let Ok(Reservation::New(reserved)) = runs.reserve(RunScope::Local, id, spec()) else {
        panic!("a new reservation");
    };
    reserved.start(
        move |log| {
            Box::pin(async move {
                let _held = held;
                for n in 0..frames {
                    let _ = log.append(n.to_string());
                }
                panic!("the loop panicked");
            })
        },
        ended,
    );
    (released, handed)
}

#[tokio::test]
async fn a_panic_before_any_event_ends_the_run_failed_and_hands_over_the_end() {
    let (runs, _, _) = registry();
    let (released, handed) = panicking(&runs, "a1", 0);

    let (frames, end) = drain(runs.events(&LOCAL, "a1", 0).unwrap()).await;

    let end = end.expect("the run ends");
    assert_eq!(end.status, RunStatus::Failed);
    assert_eq!(end.error.map(|e| e.code).as_deref(), Some("run_panicked"));
    assert!(frames.is_empty());
    assert_eq!(handed.load(Ordering::SeqCst), 0);
    assert_eq!(released.load(Ordering::SeqCst), 1, "the permit is freed");
}

#[tokio::test]
async fn a_panic_after_events_keeps_them_readable_and_hands_them_over() {
    let (runs, _, _) = registry();
    let (released, handed) = panicking(&runs, "a1", 2);

    let (frames, end) = drain(runs.events(&LOCAL, "a1", 0).unwrap()).await;

    assert_eq!(end.map(|e| e.status), Some(RunStatus::Failed));
    assert_eq!(frames.len(), 2);
    assert_eq!(handed.load(Ordering::SeqCst), 2);
    assert_eq!(released.load(Ordering::SeqCst), 1);
}

/// Chat runs share the drive, so a panicking executor ends one too.
#[tokio::test]
async fn a_chat_run_whose_executor_panics_ends_failed() {
    let (runs, executor, _) = registry();
    let script = executor.script("c1");
    runs.create(LOCAL, "c1", body("m")).unwrap();
    script.send(Cmd::Frame("a".into())).unwrap();
    script.send(Cmd::Panic).unwrap();

    let (frames, end) = drain(runs.events(&LOCAL, "c1", 0).unwrap()).await;

    assert_eq!(frames.len(), 1);
    let end = end.expect("the run ends");
    assert_eq!(end.status, RunStatus::Failed);
    assert_eq!(end.error.map(|e| e.code).as_deref(), Some("run_panicked"));
}

/// A reader is given the end only once it is handled, and a failure to
/// handle it fails the run, with its events still readable.
#[tokio::test]
async fn the_end_waits_for_its_handling_and_a_failed_handling_fails_the_run() {
    let (runs, _, _) = registry();
    let (release, released) = oneshot::channel::<()>();
    let ended: RunEnded = Box::new(move |_, _| {
        Box::pin(async move {
            let _ = released.await;
            Err(RunError {
                code: "transcript_not_saved".to_owned(),
                message: "fixed".to_owned(),
            })
        })
    });
    let Ok(Reservation::New(reserved)) = runs.reserve(RunScope::Local, "a1", spec()) else {
        panic!("a new reservation");
    };
    reserved.start(
        |log| {
            Box::pin(async move {
                let _ = log.append("one".to_owned());
                Ok(())
            })
        },
        ended,
    );
    let mut events = runs.events(&LOCAL, "a1", 0).unwrap();
    assert!(matches!(
        futures_util::StreamExt::next(&mut events).await,
        Some(RunEvent::Frame { seq: 1, .. })
    ));
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    let next = futures_util::StreamExt::next(&mut events).now_or_never();
    assert!(next.is_none(), "no end before it is handled: {next:?}");

    release.send(()).unwrap();

    let Some(Some(RunEvent::End(end))) =
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            futures_util::StreamExt::next(&mut events).await
        })
        .await
        .ok()
    else {
        panic!("the end arrives once handled");
    };
    assert_eq!(end.status, RunStatus::Failed);
    assert_eq!(
        end.error.map(|e| e.code).as_deref(),
        Some("transcript_not_saved")
    );
    let (frames, _) = drain(runs.events(&LOCAL, "a1", 0).unwrap()).await;
    assert_eq!(frames.len(), 1, "its events are still readable");
}
