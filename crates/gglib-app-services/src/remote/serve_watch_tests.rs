//! Tests for what the serve side does about the proxy it fronts: refusing an
//! enable whose proxy left while the tunnel was binding, and taking the
//! tunnel down when the proxy goes afterwards.
//!
//! Split from `enable_tests.rs`, which is at its size budget. These share
//! that file's fixture: a real proxy on a free port, and a `modelpipe::serve`
//! that binds an endpoint without reaching the network, so the whole of
//! `arm` runs here rather than stopping at the bind.

use std::sync::Arc;

use gglib_core::SettingsUpdate;
use gglib_core::events::AppEvent;
use gglib_core::services::AppCore;
use gglib_runtime::proxy::ProxyStatus;

use super::enable_tests::{Recording, ops};
use super::*;
use crate::error::GuiError;
use crate::proxy::ProxyOps;

/// A key already in settings, so `Settled::commit` has nothing to mint and
/// these two do not each spend a settings-cache window proving something
/// about the key. What they are about is the proxy.
async fn ops_with_key() -> (Arc<AppCore>, Arc<ProxyOps>, Arc<Recording>, RemoteOps) {
    let (core, proxy, events, ops) = ops().await;
    core.settings()
        .update(SettingsUpdate {
            proxy_api_key: Some(Some("already-enforced".to_owned())),
            ..SettingsUpdate::default()
        })
        .await
        .expect("settings update");
    (core, proxy, events, ops)
}

/// A request that stays off the network: no relay to reach and no discovery
/// service to publish to, so `modelpipe::serve` binds an endpoint and hands
/// back a ticket without contacting anything.
const fn offline() -> EnableRequest {
    EnableRequest {
        allow_mcp: false,
        relay: None,
        discovery: false,
    }
}

/// The proxy goes away while the tunnel is binding. `enable` takes seconds —
/// `wait_online` alone is most of them — and nothing is watching the proxy
/// across that span, because the watcher is not spawned until the install.
/// So the check before the code is granted is the only thing standing
/// between this and a pairing string for a tunnel fronting a released port.
#[tokio::test]
async fn a_proxy_that_goes_away_while_the_tunnel_binds_refuses_the_enable() {
    let (_core, proxy, events, ops) = ops_with_key().await;

    // Stopped on a state change, not on a clock. The fixture leaves the proxy
    // down, so `ensure_running` starting it is an observable transition and
    // this lands in the span between that and the check — every time, on any
    // machine.
    //
    // A `sleep` here cannot do that. The span's width is `modelpipe::serve`'s
    // `wait_online`, which is however long the endpoint takes to reach a
    // relay: measured at 3.6s on a developer machine and under 1s on CI, where
    // a one-second sleep landed *after* the check and the enable succeeded.
    // That is the test failing to make its own claim, not the code changing.
    let stopping = tokio::spawn({
        let proxy = Arc::clone(&proxy);
        async move {
            let started = tokio::time::timeout(std::time::Duration::from_secs(30), async {
                while !matches!(proxy.status().await, ProxyStatus::Running { .. }) {
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await;
            assert!(
                started.is_ok(),
                "`enable` never started the proxy it fronts"
            );
            proxy.stop().await.expect("the test proxy stops");
        }
    });

    let error = ops
        .enable(offline())
        .await
        .expect_err("a tunnel in front of a proxy that left is not enabled");
    stopping.await.expect("the stopping task");

    assert!(
        matches!(error, GuiError::Internal(ref m) if m.contains("went away while the tunnel was starting")),
        "the refusal must name what happened, got {error:?}"
    );
    assert!(
        !ops.gateway().pairing.active(),
        "a refused enable armed a pairing code anyway"
    );
    assert!(
        events.0.lock().unwrap().is_empty(),
        "a refused enable announced itself"
    );
}

/// The proxy goes away *after* the tunnel is up. The listener cannot be
/// re-pointed at another port, so the only honest answer is to stop
/// fronting it — and leaving it up is worse than having no tunnel, because
/// modelpipe 0.3.0 forwards `authorization` verbatim to whatever binds that
/// port next.
///
/// This is also what pins the watcher's *start*: it takes the slot with
/// `take_if_ours`, which passes over a reservation, and `watch_proxy` looks
/// exactly once before returning. Spawn it before the install instead of
/// after and it finds a reservation, gives up, and the tunnel below never
/// comes down.
#[tokio::test]
async fn a_tunnel_comes_down_with_the_proxy_it_fronts() {
    let (_core, proxy, events, ops) = ops_with_key().await;
    ops.enable(offline()).await.expect("the tunnel comes up");
    assert!(ops.status().await.enabled, "the tunnel is up to begin with");

    proxy.stop().await.expect("the test proxy stops");

    // Waited on the *announcement*, not on the slot. `watch_proxy` empties
    // the slot before it drains, so `enabled` goes false up to `DRAIN`
    // before the event lands, and asserting on the slot would race it.
    let announced = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let seen = events
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, AppEvent::RemoteDisabled));
            if seen {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        announced.is_ok(),
        "the tunnel is still fronting a proxy that has gone, or came down without saying so"
    );
    assert!(
        !ops.status().await.enabled,
        "the tunnel was announced as down but the slot still holds it"
    );
}

/// The window the watcher's *start* is about. `enable` mints a key here, so
/// it waits a settings-cache window after the tunnel is up and after
/// `refuse_if_gone` has already passed — several seconds in which the proxy
/// can still leave and nothing has looked since.
///
/// Either answer is correct: the check refuses the enable, or the enable
/// succeeds and the watcher takes the tunnel down behind it. What is not
/// correct is an enable that succeeds and leaves a tunnel fronting a
/// released port — which is what spawning the watcher before the install
/// produces, because `take_if_ours` passes over a reservation and
/// `watch_proxy` looks exactly once before returning.
#[tokio::test]
async fn a_proxy_that_leaves_during_the_key_wait_does_not_outlive_its_tunnel() {
    let (_core, proxy, events, ops) = ops().await;

    let stopping = tokio::spawn({
        let proxy = Arc::clone(&proxy);
        async move {
            tokio::time::sleep(std::time::Duration::from_millis(4500)).await;
            proxy.stop().await.expect("the test proxy stops");
        }
    });
    let enabled = ops.enable(offline()).await;
    stopping.await.expect("the stopping task");

    if enabled.is_err() {
        assert!(
            !ops.status().await.enabled,
            "a refused enable left a tunnel behind"
        );
        return;
    }

    let announced = tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let seen = events
                .0
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, AppEvent::RemoteDisabled));
            if seen {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await;
    assert!(
        announced.is_ok(),
        "the enable succeeded and nothing followed the proxy that had already left"
    );
}
