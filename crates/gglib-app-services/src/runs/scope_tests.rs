//! The scope rule: a device sees, reads and cancels only its own runs; this
//! machine lists and cancels every run but may not read a device's events.

use std::sync::atomic::Ordering;

use gglib_core::domain::runs::RunStatus;
use gglib_core::ports::{RunEvent, RunScope, RunsError, RunsPort};

use super::test_executor::{Cmd, body, drain, next, registry};

fn device(name: &str) -> RunScope {
    RunScope::Device(name.to_owned())
}

#[tokio::test]
async fn a_device_sees_reads_and_cancels_its_own_runs() {
    let (runs, executor, _) = registry();
    let phone = device("phone");
    let script = executor.script("p1");
    let created = runs.create(phone.clone(), "p1", body("m")).unwrap();
    assert_eq!(created.info.device.as_deref(), Some("phone"));
    script.send(Cmd::Frame("f1".into())).unwrap();

    let mut events = runs.events(&phone, "p1", 0).unwrap();
    assert!(matches!(
        next(&mut events).await,
        Some(RunEvent::Frame { seq: 1, .. })
    ));
    assert_eq!(runs.get(&phone, "p1").unwrap().id, "p1");
    assert_eq!(
        runs.cancel(&phone, "p1").unwrap().status,
        RunStatus::Cancelled
    );
}

#[tokio::test]
async fn another_device_can_neither_see_read_nor_cancel_it() {
    let (runs, _, _) = registry();
    runs.create(device("phone"), "p1", body("m")).unwrap();
    let tablet = device("tablet");

    assert_eq!(runs.get(&tablet, "p1").unwrap_err(), RunsError::NotFound);
    assert_eq!(
        runs.events(&tablet, "p1", 0).err(),
        Some(RunsError::NotFound)
    );
    assert_eq!(runs.cancel(&tablet, "p1").unwrap_err(), RunsError::NotFound);
    assert!(runs.list(&tablet).runs.is_empty());
    assert_eq!(
        runs.get(&RunScope::Local, "p1").unwrap().status,
        RunStatus::Queued,
        "the refused cancel changed nothing"
    );
}

#[tokio::test]
async fn the_same_id_from_another_scope_is_refused_without_revealing_the_run() {
    let (runs, executor, _) = registry();
    runs.create(device("phone"), "shared", body("secret-model"))
        .unwrap();

    for other in [device("tablet"), RunScope::Local] {
        let refused = runs.create(other, "shared", body("m")).unwrap_err();
        assert_eq!(refused, RunsError::IdTaken);
        assert_eq!(refused.code(), "conflict");
        assert!(!refused.to_string().contains("phone"));
        assert!(!refused.to_string().contains("secret-model"));
    }
    tokio::task::yield_now().await;
    assert_eq!(executor.started.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn this_machine_lists_and_cancels_every_run_but_may_not_read_a_devices_events() {
    let (runs, executor, _) = registry();
    let local = RunScope::Local;
    let script = executor.script("local-1");
    runs.create(device("phone"), "p1", body("m")).unwrap();
    runs.create(local.clone(), "local-1", body("m")).unwrap();

    let listed: Vec<(String, Option<String>)> = runs
        .list(&local)
        .runs
        .into_iter()
        .map(|r| (r.id, r.device))
        .collect();
    assert_eq!(
        listed,
        [
            ("local-1".to_owned(), None),
            ("p1".to_owned(), Some("phone".to_owned()))
        ]
    );
    assert!(runs.get(&local, "p1").is_ok());
    let refused = runs.events(&local, "p1", 0).err().expect("refused");
    assert_eq!(refused, RunsError::NotYours);
    assert_eq!((refused.code(), refused.http_status()), ("not_yours", 403));
    assert_eq!(
        runs.cancel(&local, "p1").unwrap().status,
        RunStatus::Cancelled
    );

    script.send(Cmd::Finish(Ok(()))).unwrap();
    let (_, end) = drain(runs.events(&local, "local-1", 0).unwrap()).await;
    assert_eq!(end.map(|i| i.status), Some(RunStatus::Completed));
    assert_eq!(runs.list(&device("phone")).runs.len(), 1);
}

#[tokio::test]
async fn forgetting_a_device_cancels_and_drops_its_runs_and_ends_their_readers() {
    let (runs, executor, _) = registry();
    let phone = device("phone");
    let script = executor.script("p1");
    runs.create(phone.clone(), "p1", body("m")).unwrap();
    runs.create(phone.clone(), "p2", body("m")).unwrap();
    runs.create(device("tablet"), "t1", body("m")).unwrap();
    runs.create(RunScope::Local, "l1", body("m")).unwrap();
    script.send(Cmd::Frame("f1".into())).unwrap();
    let mut events = runs.events(&phone, "p1", 0).unwrap();
    next(&mut events).await;

    assert_eq!(runs.forget_device("phone"), 2);

    assert_eq!(next(&mut events).await, None, "the reader ends");
    assert!(runs.list(&phone).runs.is_empty());
    let left: Vec<String> = runs
        .list(&RunScope::Local)
        .runs
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(left, ["l1", "t1"]);
    assert_eq!(runs.forget_device("phone"), 0);
}
