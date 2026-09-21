//! Tests for when the tunnel stops fronting the proxy it was built for.
//!
//! This file used to restate modelpipe's locality verdicts, case by case,
//! because `locality::admits` was `pub(crate)` over there and this side could
//! only predict it. [`BackendUrl`] carries the verdict now and modelpipe
//! tests it, so those cases are gone rather than duplicated. What is left is
//! gglib's own question — whether the proxy on the other end of this address
//! is still the one being fronted — plus the single thing the migration can
//! silently get wrong.

use std::net::SocketAddr;

use super::*;
use crate::remote::slot::Slot;
use crate::test_support::test_core_and_proxy;

fn addr(s: &str) -> SocketAddr {
    s.parse().expect("test address")
}

/// The address every watcher test uses, as a bind and as the backend the
/// tunnel was built against. Loopback, so `BackendUrl::at` leaves it alone and
/// the tests are about the watching rather than about the rewrite.
const BOUND: &str = "127.0.0.1:8080";

/// A watcher's world, minus the tunnel it fronts.
///
/// The proxy comes from [`test_core_and_proxy`], which never starts one, so
/// `status()` answers `Stopped` — the same answer a crashed or aborted proxy
/// leaves behind, and the reason these tests need no listener. The sender is
/// returned so it stays alive: dropping it closes the channel, which
/// [`until_gone`] correctly reads as the process coming down.
fn watched() -> (
    watch::Sender<ProxyStatus>,
    watch::Receiver<ProxyStatus>,
    BackendUrl,
    CancellationToken,
) {
    let (tx, rx) = watch::channel(ProxyStatus::Running {
        address: addr(BOUND),
    });
    (
        tx,
        rx,
        BackendUrl::at(addr(BOUND)),
        CancellationToken::new(),
    )
}

/// A `Live` a test can build: a real `modelpipe::ServeHandle` exists nowhere
/// but in front of a listener, and [`take_if_ours`] never looks inside one.
fn live_on(handle: &Arc<u8>) -> Live<u8> {
    Live {
        handle: Arc::clone(handle),
        cancel: CancellationToken::new(),
        epoch: 1,
    }
}

// ── The permission the address carries ──────────────────────────────────

/// Options for a listener that reaches no network of its own: no relay to
/// find and no discovery service to publish to.
fn offline() -> modelpipe::ServeOptions {
    let mut opts = modelpipe::ServeOptions::default();
    opts.auth = modelpipe::TokenPolicy::Named;
    opts.discovery = false;
    opts.port_mapping = false;
    opts
}

/// The one thing gglib still has to get right about the backend now that
/// modelpipe decides the rest: [`BackendUrl::at`] carries a LAN address's
/// permission to be dialled, and handing `serve` a URL *string* does not.
///
/// **What this does and does not cover.** It pins modelpipe's asymmetry,
/// not gglib's call site: watching `arm` choose `at` would take a real
/// LAN-bound proxy and a real endpoint, which this suite has neither of.
/// What protects the call site is structural — with `Backend` and its `url`
/// field gone there is no string to hand over by accident, and
/// reintroducing the bug means writing `.url()` on purpose. This test is
/// here so the hazard is written down, and so it fails loudly if a later
/// modelpipe makes a bare URL permissive and quietly stops mattering.
///
/// A LAN literal with nothing behind it is enough: `serve` classifies the
/// address before it dials it, so the refusal needs no listener on the far
/// side, and the whole case runs in milliseconds.
#[tokio::test]
async fn a_lan_address_is_permitted_by_at_and_not_by_a_bare_url() {
    const LAN: &str = "192.168.1.5:8080";

    let refused = modelpipe::serve(format!("http://{LAN}"), offline()).await;
    assert!(
        matches!(refused, Err(modelpipe::ServeError::BackendNotLocal { .. })),
        "a bare URL converts through `BackendUrl::dial`, which permits no \
         private address"
    );

    let served = modelpipe::serve(BackendUrl::at(addr(LAN)), offline())
        .await
        .expect("`BackendUrl::at` carries the permission the same address needs");
    served.shutdown().await;
}

// ── When the tunnel stops fronting its proxy ─────────────────────────────

/// Both exits count. `POST /api/proxy/stop` publishes `Stopped` and a proxy
/// task that fell over publishes `Crashed`; reacting only to the crash would
/// leave the deliberate stop — the one a person just asked for — with a
/// tunnel still forwarding into a port nobody owns.
#[test]
fn both_ways_the_proxy_exits_take_the_tunnel_down() {
    let backend = BackendUrl::at(addr("127.0.0.1:8080"));
    assert!(!still_fronting(&ProxyStatus::Stopped, &backend));
    assert!(!still_fronting(&ProxyStatus::Crashed, &backend));
}

/// The ordinary poll: the proxy is up on the address the tunnel dials, so
/// nothing happens. This is the answer several times a minute for the whole
/// life of a session, and it must not cost the tunnel anything.
#[test]
fn a_proxy_still_on_the_address_the_tunnel_dials_is_left_alone() {
    let backend = BackendUrl::at(addr("127.0.0.1:8080"));
    let status = ProxyStatus::Running {
        address: addr("127.0.0.1:8080"),
    };
    assert!(still_fronting(&status, &backend));
}

/// The address is re-read on every poll, and the rewrite is applied to what
/// comes back — otherwise a proxy on the wildcard would compare its own
/// `0.0.0.0:8080` against the `127.0.0.1:8080` the tunnel was given and read
/// as a stranger, tearing down a healthy tunnel every five seconds.
#[test]
fn a_wildcard_bind_still_matches_the_loopback_address_it_was_rewritten_to() {
    let backend = BackendUrl::at(addr("0.0.0.0:8080"));
    let status = ProxyStatus::Running {
        address: addr("0.0.0.0:8080"),
    };
    assert!(still_fronting(&status, &backend));
}

/// A proxy that went away and came back on another port is running, and is
/// not this tunnel's backend. `modelpipe::serve` still holds the old port, so
/// leaving the tunnel up would forward to whatever holds it now.
#[test]
fn a_proxy_that_came_back_on_another_port_is_not_the_one_being_fronted() {
    let backend = BackendUrl::at(addr("127.0.0.1:8080"));
    let status = ProxyStatus::Running {
        address: addr("127.0.0.1:9099"),
    };
    assert!(!still_fronting(&status, &backend));
}

// ── The watcher's two arms ───────────────────────────────────────────────

/// The exit nothing announces, which is the whole reason the poll exists.
/// `exit_tx.send` runs *inside* the proxy task after the serve future
/// returns, so a task that panicked never reaches it and one that
/// `ProxySupervisor::stop` aborted on its five-second timeout is killed
/// before it. Both drop the listener and release the port. Not one value is
/// sent on the channel here, and the tunnel still comes down.
///
/// Time is paused, so the wait costs nothing; the outer timeout is what
/// turns "the poll arm is gone" into a failure instead of a hang.
#[tokio::test]
async fn a_proxy_exit_that_publishes_nothing_is_still_noticed() {
    let (_core, proxy) = test_core_and_proxy().await;
    let (_tx, mut exit, backend, cancel) = watched();
    // Paused here rather than for the whole test: the in-memory database is
    // built on a real clock, and auto-advance would run straight through the
    // connection pool's own timeout.
    tokio::time::pause();

    let gone = tokio::time::timeout(
        Duration::from_secs(60),
        until_gone(&proxy, &mut exit, &cancel, &backend),
    )
    .await
    .expect("an exit nothing published must still be noticed");
    assert!(gone);
}

/// The fast path. The channel arm is what makes an ordinary exit immediate
/// rather than up to `PROXY_POLL` late, so the wait is bounded well under
/// that: on the poll alone this would take five seconds.
#[tokio::test]
async fn an_exit_published_on_the_channel_is_acted_on_without_waiting_for_the_poll() {
    let (_core, proxy) = test_core_and_proxy().await;
    let (tx, mut exit, backend, cancel) = watched();
    tx.send(ProxyStatus::Stopped)
        .expect("the receiver is alive");

    let gone = tokio::time::timeout(
        Duration::from_millis(250),
        until_gone(&proxy, &mut exit, &cancel, &backend),
    )
    .await
    .expect("the channel arm must not wait for the poll");
    assert!(gone);
}

/// `disable` ran: the token it shares with the rotation poll is cancelled,
/// and this watcher has nothing left to undo. Answering `false` is what
/// keeps it from racing `disable` for the same handle.
#[tokio::test]
async fn a_cancelled_watcher_stops_without_taking_anything_down() {
    let (_core, proxy) = test_core_and_proxy().await;
    let (_tx, mut exit, backend, cancel) = watched();
    cancel.cancel();

    let gone = tokio::time::timeout(
        Duration::from_millis(250),
        until_gone(&proxy, &mut exit, &cancel, &backend),
    )
    .await
    .expect("a cancelled watcher must not wait for anything");
    assert!(!gone, "cancellation is not the proxy going away");
}

// ── Which tunnel a watcher may take down ─────────────────────────────────

/// The identity guard. Between the proxy exiting and this lock, a `disable`
/// and a fresh `enable` can have swapped in a different tunnel — and taking
/// *that* one down would unpair every machine paired with it, silently, over
/// a proxy exit that had nothing to do with it.
///
/// The two handles hold the same value on purpose: what separates them is
/// the allocation, so a guard that compared anything but the pointer would
/// call these one tunnel.
#[test]
fn a_watcher_only_takes_down_the_tunnel_it_was_spawned_beside() {
    let mine = Arc::new(1_u8);
    let someone_elses = Arc::new(1_u8);

    let mut slot = Slot::Full(live_on(&someone_elses));
    assert!(
        take_if_ours(&mut slot, &mine).is_none(),
        "a tunnel this watcher never fronted must be left alone"
    );
    assert!(
        slot.full().is_some(),
        "and left where it was, not dropped on the floor"
    );

    assert!(
        take_if_ours(&mut slot, &someone_elses).is_some(),
        "its own tunnel is the one it takes"
    );
    assert!(
        slot.full().is_none(),
        "and takes out, so `disable` finds nothing"
    );
}

/// `disable` got here first. Nothing to match, nothing to undo.
#[test]
fn a_tunnel_already_taken_down_leaves_the_watcher_nothing_to_do() {
    let mut slot: Slot<Live<u8>> = Slot::Empty;
    assert!(take_if_ours(&mut slot, &Arc::new(1_u8)).is_none());
}
