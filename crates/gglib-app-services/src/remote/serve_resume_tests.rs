//! Tests for an `enable`, `invite` or `disable` that arrives while the
//! daemon's own startup resume is putting the tunnel back ([#1037]), against
//! a real arm.
//!
//! On `serve_watch_tests.rs`'s fixture: a real proxy on a free port, and a
//! `modelpipe::serve` that binds without reaching the network, so the resume
//! really arms and really holds the slot while it does. Each test holds the
//! serve slot's lock while it starts the resume, so the resume has said it is
//! working and cannot get past its first look at the slot. The call is made,
//! seen to be waiting, and only then is the lock let go.
//!
//! **Everything fallible is asserted after the cleanup**, for
//! `serve_invite_tests.rs`'s reason: a key minted here is a real row in the
//! key file until it is forgotten.
//!
//! [#1037]: https://github.com/mmogr/gglib/issues/1037

use std::sync::Arc;
use std::time::Duration;

use gglib_core::services::AppCore;
use gglib_core::{RemoteServe, SettingsUpdate};
use tokio::sync::OwnedMutexGuard;

use super::serve_watch_tests::{offline, ops_with_key};
use super::*;

/// The state `enable` leaves behind, which is what a resume arms from.
async fn switched_on(core: &AppCore, relay: Option<&str>) {
    core.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            remote_serve: Some(Some(RemoteServe {
                allow_mcp: false,
                relay: relay.map(str::to_owned),
                discovery: false,
            })),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the switch is recorded");
}

/// The daemon's resume, started while this holds the serve slot's lock: it
/// has said it is working, and cannot get past its first look at the slot
/// until the guard is dropped.
async fn a_resume_held_at_the_slot(
    ops: &Arc<RemoteOps>,
) -> (tokio::task::JoinHandle<()>, OwnedMutexGuard<Slot<Live>>) {
    let held = Arc::clone(&ops.live).lock_owned().await;
    let resume = tokio::spawn({
        let ops = Arc::clone(ops);
        async move { ops.resume().await }
    });
    let started = tokio::time::timeout(Duration::from_secs(10), async {
        while !*ops.resuming.subscribe().borrow() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(started.is_ok(), "the resume never said it was working");
    (resume, held)
}

/// Spawn a call, and see that it is waiting for the resume rather than
/// answering; the caller lets the resume go after this.
async fn waiting<T: Send + 'static>(
    call: impl std::future::Future<Output = T> + Send + 'static,
) -> tokio::task::JoinHandle<T> {
    let mut task = tokio::spawn(call);
    assert!(
        tokio::time::timeout(Duration::from_millis(200), &mut task)
            .await
            .is_err(),
        "the call answered while the resume was still working"
    );
    task
}

/// What the report was about: `enable --invite` typed at a daemon that is
/// putting its tunnel back. It gets a code on the session the resume armed.
#[tokio::test]
async fn an_enable_that_arrives_during_the_startup_resume_pairs_a_device_onto_it_instead_of_refusing()
 {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    switched_on(&core, None).await;
    let ops = Arc::new(ops);
    let (resume, held) = a_resume_held_at_the_slot(&ops).await;
    let enabling = waiting({
        let ops = Arc::clone(&ops);
        async move {
            ops.enable(EnableRequest {
                invite: true,
                ..offline()
            })
            .await
        }
    })
    .await;
    drop(held);
    let enabled = enabling.await.expect("the enable task");
    let _ = resume.await;

    // Clean up before judging.
    let forgot = match &enabled {
        Ok(Enabled {
            pairing: Some(offered),
            ..
        }) => Some(ops.forget(&offered.device).await),
        _ => None,
    };
    let stopped = ops.disable().await;

    let enabled = enabled.expect("an enable during a resume is answered, not refused");
    assert!(
        enabled.already_up,
        "answered by the session the resume armed"
    );
    assert!(enabled.pairing.is_some(), "with the code it asked for");
    assert!(
        matches!(forgot, Some(Ok(true))),
        "the device it minted was forgotten: {forgot:?}"
    );
    stopped.expect("disable");
}

/// A plain `enable` that waited is answered by the session it waited for.
/// It was not up when the person asked, and it is now, which is what they
/// asked for; no code, because none was asked for.
#[tokio::test]
async fn a_plain_enable_that_waited_out_the_resume_is_answered_by_the_session_it_waited_for() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    switched_on(&core, None).await;
    let ops = Arc::new(ops);
    let (resume, held) = a_resume_held_at_the_slot(&ops).await;
    let enabling = waiting({
        let ops = Arc::clone(&ops);
        async move { ops.enable(offline()).await }
    })
    .await;
    drop(held);
    let enabled = enabling.await.expect("the enable task");
    let _ = resume.await;
    let up = ops.status().await.enabled;
    let stopped = ops.disable().await;

    let enabled = enabled.expect("a plain enable during a resume is answered, not refused");
    assert!(
        enabled.already_up,
        "answered by the session the resume armed"
    );
    assert!(enabled.pairing.is_none(), "a plain enable offers no code");
    assert!(up, "and the session it answered from is up");
    stopped.expect("disable");
}

/// `gglib remote invite` a few seconds after the daemon started gets the
/// same answer `enable --invite` does: a code, on the session that came back.
#[tokio::test]
async fn an_invite_that_arrives_during_the_startup_resume_offers_a_code_on_it() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    switched_on(&core, None).await;
    let ops = Arc::new(ops);
    let (resume, held) = a_resume_held_at_the_slot(&ops).await;
    let inviting = waiting({
        let ops = Arc::clone(&ops);
        async move { ops.invite().await }
    })
    .await;
    drop(held);
    let invited = inviting.await.expect("the invite task");
    let _ = resume.await;

    let forgot = match &invited {
        Ok(Enabled {
            pairing: Some(offered),
            ..
        }) => Some(ops.forget(&offered.device).await),
        _ => None,
    };
    let stopped = ops.disable().await;

    let invited = invited.expect("an invite during a resume is answered, not refused");
    assert!(invited.pairing.is_some(), "with a code");
    assert!(
        matches!(forgot, Some(Ok(true))),
        "the device it minted was forgotten: {forgot:?}"
    );
    stopped.expect("disable");
}

/// A resume that arms nothing leaves the slot empty when it ends, here
/// because the relay it was left with is not a relay URL at all, which
/// modelpipe refuses before binding. The `enable` that waited for it then
/// arms the tunnel itself, with its own flags.
#[tokio::test]
async fn an_enable_that_waited_on_a_resume_that_armed_nothing_arms_the_tunnel_itself() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    switched_on(&core, Some("not a relay url")).await;
    let ops = Arc::new(ops);
    let (resume, held) = a_resume_held_at_the_slot(&ops).await;
    let enabling = waiting({
        let ops = Arc::clone(&ops);
        async move { ops.enable(offline()).await }
    })
    .await;
    drop(held);
    let enabled = enabling.await.expect("the enable task");
    let _ = resume.await;
    let stopped = ops.disable().await;

    let enabled = enabled.expect("the enable armed the tunnel the resume could not");
    assert!(
        !enabled.already_up,
        "this call armed the session, so its flags are the session's"
    );
    stopped.expect("disable");
}

/// A `disable` that lands while the resume is still starting the proxy finds
/// the slot empty and nothing to cancel. The resume notices it when it
/// reserves the slot, gives the slot back, and leaves the switch off: it no
/// longer writes back the switch it read a moment before.
///
/// The short pause lets the resume get past reading the switch before the
/// `disable` writes it. If the resume has not got that far, it reads the
/// switch off and stops there instead, and the outcome asserted is the same;
/// the pause can only make this test weaker, never make it fail.
#[tokio::test]
async fn a_disable_during_the_resume_stops_it_and_the_switch_stays_off() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    switched_on(&core, None).await;
    let ops = Arc::new(ops);
    let (resume, held) = a_resume_held_at_the_slot(&ops).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let disabling = tokio::spawn({
        let ops = Arc::clone(&ops);
        async move { ops.disable().await }
    });
    // The switch is written before the slot is looked at, so once it reads
    // off, the `disable` is queued behind the lock this test holds.
    let wrote = tokio::time::timeout(Duration::from_secs(10), async {
        while core
            .settings()
            .get()
            .await
            .ok()
            .and_then(|s| s.remote_enabled)
            != Some(false)
        {
            tokio::task::yield_now().await;
        }
    })
    .await;
    drop(held);
    let _ = disabling.await;
    let _ = resume.await;
    let up = ops.status().await.enabled;
    let switch = core.settings().get().await.map(|s| s.remote_enabled);
    // In case the resume armed after all: take it down before judging.
    let _ = ops.disable().await;

    assert!(wrote.is_ok(), "the disable never wrote the switch");
    assert!(!up, "the resume armed a tunnel after a disable");
    assert_eq!(
        switch.ok().flatten(),
        Some(false),
        "the resume turned the switch back on"
    );
}
