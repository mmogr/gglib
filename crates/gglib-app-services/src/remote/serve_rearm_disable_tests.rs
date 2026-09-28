//! Tests for a `disable` racing an arm from the switch: a look of the
//! daemon's follower, and the startup resume, which reserves the serve slot
//! through the same check in `turn_on`.
//!
//! A `disable` says so on `disables`, writes the switch off, says so again,
//! and takes the slot. An arm from the switch subscribes before it reads the
//! switch. Each test parks the reads that put the two in the order it is
//! about, on `serve_rearm_look_tests.rs`'s store double, so none needs a
//! clock.
//!
//! **Everything fallible is asserted after the cleanup**, for
//! `serve_rearm_tests.rs`'s reason.

use std::sync::Arc;

use tokio::task::JoinHandle;

use super::serve_rearm_look_tests::{looking, ops_over_a_parked_store};
use super::*;

/// A `disable`, spawned.
fn disabling(ops: &Arc<RemoteOps>) -> JoinHandle<Result<(), GuiError>> {
    let ops = Arc::clone(ops);
    tokio::spawn(async move { ops.disable().await })
}

/// Whether `disable` found the slot empty: nothing had reserved it yet.
fn found_nothing(disabled: &Result<(), GuiError>) -> bool {
    matches!(disabled, Err(GuiError::Conflict(m)) if m == "remote access is not enabled")
}

/// A `disable` that lands after the look has read the switch on and before
/// it reserves the slot finds nothing to cancel, so the look has to see it
/// itself. It can only because it subscribed to `disables` before it read.
#[tokio::test]
async fn a_disable_between_the_switch_read_and_the_reservation_wins() {
    let (core, parked, proxy, ops, _arming) = ops_over_a_parked_store().await;
    // The look's read of the switch is the first.
    parked.park([1, 0]);
    let look = looking(&ops);
    let reached = parked.gates[0].reached().await;
    // "Not enabled", since nothing is bound; the switch is written off first.
    let _ = ops.disable().await;
    parked.gates[0].open();
    let decision = look.await.expect("the look");
    let up = ops.status().await.enabled;
    let switch = core.settings().get().await.map(|s| s.remote_enabled);

    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(reached, "the look never read the switch");
    assert_eq!(decision, Rearm::SwitchOff);
    assert!(!up, "the look armed after a disable");
    assert_eq!(switch.ok().flatten(), Some(false), "the switch is on again");
}

/// The other order: the `disable` has said so, the look subscribes after
/// that, and it reads the switch before the `disable`'s write lands, so it
/// reads it on; the `disable` then finds nothing to take. The look hears the
/// `disable` at its reservation all the same, because `disable` says so a
/// second time once its write is over.
#[tokio::test]
async fn a_disable_whose_write_lands_after_the_switch_read_wins() {
    let (core, parked, proxy, ops, _arming) = ops_over_a_parked_store().await;
    // The `disable`'s write reads first, and the look's read of the switch
    // is the second.
    parked.park([1, 2]);
    let disable = disabling(&ops);
    let writing = parked.gates[0].reached().await;
    let look = looking(&ops);
    let read = parked.gates[1].reached().await;
    parked.gates[0].open();
    let disabled = disable.await.expect("the disable");
    parked.gates[1].open();
    let decision = look.await.expect("the look");
    let up = ops.status().await.enabled;
    let switch = core.settings().get().await.map(|s| s.remote_enabled);

    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(writing, "the disable never reached its write");
    assert!(read, "the look never read the switch");
    assert!(found_nothing(&disabled), "{disabled:?}");
    assert_eq!(decision, Rearm::SwitchOff);
    assert!(!up, "the look armed after a disable");
    assert_eq!(switch.ok().flatten(), Some(false), "the switch is on again");
}

/// The same order against the startup resume, which has no decision to
/// return: it has run to its end, and nothing is serving.
#[tokio::test]
async fn a_disable_whose_write_lands_after_the_resume_read_the_switch_wins() {
    let (_core, parked, proxy, ops, _arming) = ops_over_a_parked_store().await;
    // The `disable`'s write reads first, and the resume's read of the switch
    // is the second.
    parked.park([1, 2]);
    let disable = disabling(&ops);
    let writing = parked.gates[0].reached().await;
    let resume = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.resume().await }
    });
    let read = parked.gates[1].reached().await;
    parked.gates[0].open();
    let disabled = disable.await.expect("the disable");
    parked.gates[1].open();
    let resumed = resume.await.is_ok();
    let up = ops.status().await.enabled;

    let _ = ops.disable().await;
    let _ = proxy.stop().await;

    assert!(writing, "the disable never reached its write");
    assert!(read, "the resume never read the switch");
    assert!(found_nothing(&disabled), "{disabled:?}");
    assert!(resumed, "the resume did not run to its end");
    assert!(!up, "the resume armed after a disable");
}
