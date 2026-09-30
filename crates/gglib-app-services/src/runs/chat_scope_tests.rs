//! A run on one of the hub's chats belongs to the chat: this machine and
//! every paired device see, read and cancel it, whoever started it, and
//! forgetting the device that started it leaves it running and saved. A
//! device's own chat run stays its own.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gglib_core::domain::runs::{RunError, RunKind, RunStatus};
use gglib_core::ports::{RunEvent, RunScope, RunsError, RunsPort};
use tokio::sync::oneshot;

use super::RunRegistry;
use super::cell::RunSpec;
use super::local::{Reservation, RunEnded};
use super::test_executor::{body, drain, next, registry, until};

fn device(name: &str) -> RunScope {
    RunScope::Device(name.to_owned())
}

/// Start `scope`'s agent run `id` on chat `chat`: it logs one frame and
/// ends when `finish` is sent; `saved` is set once its end is handled.
fn on_chat(
    runs: &RunRegistry,
    scope: RunScope,
    id: &str,
    chat: i64,
) -> (oneshot::Sender<()>, Arc<AtomicBool>) {
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: Some(chat),
    };
    let (finish, finished) = oneshot::channel::<()>();
    let saved = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&saved);
    let ended: RunEnded = Box::new(move |_, _| {
        Box::pin(async move {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        })
    });
    let Ok(Reservation::New(reserved)) = runs.reserve(scope, id, spec) else {
        panic!("a new reservation");
    };
    reserved.start(
        |log| {
            Box::pin(async move {
                log.started();
                let _ = log.append("{\"type\":\"text_delta\",\"content\":\"x\"}".to_owned());
                let _ = finished.await;
                Ok::<(), RunError>(())
            })
        },
        ended,
    );
    (finish, saved)
}

#[tokio::test]
async fn this_machine_reads_a_devices_run_on_a_hub_chat() {
    let (runs, _, _) = registry();
    let (finish, _) = on_chat(&runs, device("phone"), "d1", 7);
    let local = RunScope::Local;
    assert_eq!(runs.list(&local).runs.len(), 1);
    let mut events = runs.events(&local, "d1", 0).unwrap();
    assert!(matches!(
        next(&mut events).await,
        Some(RunEvent::Frame { seq: 1, .. })
    ));
    finish.send(()).unwrap();
    let (_, end) = drain(events).await;
    assert_eq!(end.map(|i| i.status), Some(RunStatus::Completed));
}

#[tokio::test]
async fn another_device_sees_reads_and_cancels_a_run_on_a_hub_chat() {
    let (runs, _, _) = registry();
    let (_finish, _) = on_chat(&runs, device("phone"), "d1", 7);
    let (_local_finish, _) = on_chat(&runs, RunScope::Local, "l1", 8);
    let laptop = device("laptop");

    let mut seen: Vec<String> = runs.list(&laptop).runs.into_iter().map(|r| r.id).collect();
    seen.sort();
    assert_eq!(seen, ["d1", "l1"], "the hub's own run on a chat too");
    assert_eq!(
        runs.get(&laptop, "d1").unwrap().device.as_deref(),
        Some("phone")
    );
    let mut events = runs.events(&laptop, "d1", 0).unwrap();
    assert!(matches!(
        next(&mut events).await,
        Some(RunEvent::Frame { seq: 1, .. })
    ));
    assert!(runs.events(&laptop, "l1", 0).is_ok());
    assert_eq!(
        runs.cancel(&laptop, "d1").unwrap().status,
        RunStatus::InProgress,
        "cancelled, and shown going until its end is handled"
    );
}

/// The contrast: a device's own chat run, with no hub chat, is its alone.
#[tokio::test]
async fn another_device_still_cannot_reach_a_devices_own_chat_run() {
    let (runs, _, _) = registry();
    runs.create(device("phone"), "p1", body("m")).unwrap();
    let laptop = device("laptop");
    assert_eq!(runs.get(&laptop, "p1").unwrap_err(), RunsError::NotFound);
    assert_eq!(
        runs.events(&laptop, "p1", 0).err(),
        Some(RunsError::NotFound)
    );
    assert_eq!(
        runs.events(&RunScope::Local, "p1", 0).err(),
        Some(RunsError::NotYours)
    );
    assert!(runs.list(&laptop).runs.is_empty());
}

/// Forgetting a device drops its own runs, but its run on a hub chat is the
/// chat's: it goes on and its reply is saved.
#[tokio::test]
async fn forgetting_the_device_leaves_its_run_on_a_hub_chat_going_and_saved() {
    let (runs, _, _) = registry();
    let (finish, saved) = on_chat(&runs, device("phone"), "d1", 7);
    runs.create(device("phone"), "p1", body("m")).unwrap();

    assert_eq!(runs.forget_device("phone"), 1, "only its own chat run");
    let left: Vec<String> = runs
        .list(&RunScope::Local)
        .runs
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(left, ["d1"]);
    let status = runs.get(&RunScope::Local, "d1").unwrap().status;
    assert!(!status.is_terminal(), "still going: {status:?}");

    finish.send(()).unwrap();
    until(|| saved.load(Ordering::SeqCst)).await;
    until(|| runs.get(&RunScope::Local, "d1").unwrap().status == RunStatus::Completed).await;
}
