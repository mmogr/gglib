//! Reading a run on one of the hub's chats as someone other than the device
//! that started it: the real error text, and no hand in its retention. A
//! device's own chat run still gives others the fixed text.

use gglib_core::domain::runs::{RunError, RunKind, RunStatus};
use gglib_core::ports::{RunScope, RunsError, RunsPort};

use super::RunRegistry;
use super::cell::RunSpec;
use super::local::{Reservation, RunEnded};
use super::registry::OTHERS_MESSAGE;
use super::test_executor::{drain, registry, until};

const MINUTE: u64 = 60 * 1000;

fn device(name: &str) -> RunScope {
    RunScope::Device(name.to_owned())
}

/// `scope`'s agent run `id` on chat 7 (or none), ended with `outcome` and
/// its end handled.
async fn ended(
    runs: &RunRegistry,
    scope: RunScope,
    id: &str,
    chat: Option<i64>,
    outcome: Result<(), RunError>,
) {
    let spec = RunSpec {
        kind: RunKind::Agent,
        model: None,
        conversation_id: chat,
    };
    let Ok(Reservation::New(reserved)) = runs.reserve(scope, id, spec) else {
        panic!("a new reservation");
    };
    let handled: RunEnded = Box::new(|_, _| Box::pin(async { Ok(()) }));
    reserved.start(|_| Box::pin(async move { outcome }), handled);
    until(|| {
        runs.get(&RunScope::Local, id)
            .is_ok_and(|info| info.status.is_terminal())
    })
    .await;
}

fn failure() -> RunError {
    RunError {
        code: "agent_error".to_owned(),
        message: "PRIVATE-FAILURE-TEXT".to_owned(),
    }
}

#[tokio::test]
async fn a_failed_run_on_a_hub_chat_gives_every_reader_its_error() {
    let (runs, _, _) = registry();
    ended(&runs, device("phone"), "d1", Some(7), Err(failure())).await;
    for reader in [RunScope::Local, device("laptop"), device("phone")] {
        let info = runs.get(&reader, "d1").unwrap();
        assert_eq!(info.status, RunStatus::Failed);
        assert_eq!(info.error, Some(failure()), "{reader:?}");
    }
}

#[tokio::test]
async fn a_devices_own_failed_run_gives_this_machine_the_fixed_text() {
    let (runs, _, _) = registry();
    ended(&runs, device("phone"), "p1", None, Err(failure())).await;
    let error = runs.get(&RunScope::Local, "p1").unwrap().error.unwrap();
    assert_eq!(error.code, "agent_error");
    assert_eq!(error.message, OTHERS_MESSAGE);
}

/// Only the device that started the run starts its ten minutes by reading
/// it to the end; this machine and another device reading it do not.
#[tokio::test]
async fn only_the_starting_device_reading_it_through_starts_the_short_retention() {
    let (runs, _, clock) = registry();
    ended(&runs, device("phone"), "d1", Some(7), Ok(())).await;

    for reader in [RunScope::Local, device("laptop")] {
        drain(runs.events(&reader, "d1", 0).unwrap()).await;
    }
    clock.advance(10 * MINUTE);
    assert!(
        runs.get(&RunScope::Local, "d1").is_ok(),
        "others' reads keep it"
    );

    drain(runs.events(&device("phone"), "d1", 0).unwrap()).await;
    clock.advance(10 * MINUTE);
    assert_eq!(
        runs.get(&RunScope::Local, "d1").unwrap_err(),
        RunsError::NotFound
    );
}
