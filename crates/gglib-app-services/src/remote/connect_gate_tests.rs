//! The gate between binding the port and using it, at its call site.
//!
//! modelpipe owns the policy now: `ConnectHandle::wait_reachable` waits for
//! the far machine, and `pair` presents a code only once it has. This file
//! asserts what is still `dial`'s, which no test inside modelpipe can: that
//! it goes through them, and what `disconnect` may end while it does.
//!
//! Its own file rather than more of `connect_race_tests.rs`, which is at
//! 229 of the 300 lines `scripts/check_rust_complexity.sh` allows and would
//! join the ratchet's baseline rather than pass it. The subject is a
//! different one anyway: that file is about two commands arriving at once,
//! this one is about the order of two steps inside one.

use std::net::Ipv4Addr;
use std::time::Duration;

use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use super::*;
use crate::test_support_remote::{KEY_A, TICKET_UNREACHABLE, paired_with, test_remote_ops};

/// What `gglib remote disconnect` allows the daemon before it gives up
/// (`gglib-cli/src/daemon_client/remote.rs` gives it fifteen; five is
/// stricter and still far more than an answer needs).
const PROMPTLY: Duration = Duration::from_secs(5);

/// How long a dial that `disconnect` ended may take to finish ending.
///
/// Not [`PROMPTLY`]: `disconnect` answers at once, but the dial it ended
/// first drains the listener it bound, for up to `DRAIN`, five seconds, so a
/// five-second wait here measured the machine's load as much as the dial
/// (#1078). Four drains is room for a loaded machine, and still under the
/// thirty seconds a dial that ignored `disconnect` would spend waiting for
/// the far machine, which is the failure this wait exists to catch.
const ENDED: Duration = Duration::from_secs(4 * DRAIN.as_secs());

/// How long to watch a dial with a code after `disconnect` before judging
/// that it is still pairing.
///
/// A dial that `disconnect` wrongly abandoned is over once it has torn its
/// endpoint down, and under load that took longer than the half second this
/// used to wait, so the mutation it exists to catch sometimes survived. A
/// dial that honours its code waits twenty-five seconds for the far machine,
/// so five cannot mistake one for the other.
const ABANDONED: Duration = Duration::from_secs(5);

/// A dial with a code is not abandoned part way through pairing, and
/// `disconnect` still answers at once.
///
/// `modelpipe::pair` binds the port, waits for the far machine and presents
/// the code in one call, and nothing on this side can see which of those it
/// is in. Abandoning it could spend the code on a pairing nobody collects,
/// and the recovery for that is a walk to the other machine. So `disconnect`
/// takes the slot and returns, and the dial runs on until `pair` answers; it
/// then finds the slot gone and takes its port down. Before modelpipe owned
/// pairing a coded dial was cancellable while it waited, because gglib
/// redeemed the code itself afterwards.
///
/// It needs no network, for the reason `a_dial_that_fails_gives_the_connect_side_back`
/// needs none: the local bind is the first thing `pair` does, and the iroh
/// endpoint binds its own socket without anything answering.
///
/// **The mutation it kills** is the coded arm wrapped in the same `select!`
/// on the cancel token as the codeless one: the dial then ends the moment
/// `disconnect` takes the slot.
#[tokio::test(flavor = "multi_thread")]
async fn a_dial_with_a_code_is_not_abandoned_part_way_through_pairing() {
    let (_, ops, events) = test_remote_ops().await;
    // Named by binding it and letting it go, so the dial below can be
    // watched for the moment it takes the same port.
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("a free loopback port")
        .local_addr()
        .expect("the port just bound")
        .port();

    let dialling = Arc::clone(&ops);
    let dial = tokio::spawn(async move {
        dialling
            .join(JoinRequest {
                pairing: Some(format!("{TICKET_UNREACHABLE}-483920")),
                port: Some(port),
                discovery: false,
                ..JoinRequest::default()
            })
            .await
    });
    parked(port, &dial).await;

    tokio::time::timeout(PROMPTLY, ops.disconnect())
        .await
        .expect("disconnect queued behind the dial it was asked to end")
        .expect("a dial in flight is one to give up on");
    // Watched rather than glanced at: an abandoned dial still has to tear its
    // endpoint down before it is over.
    let _ = tokio::time::timeout(ABANDONED, async {
        while !dial.is_finished() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    let still_pairing = !dial.is_finished();
    // The far machine does not exist, so `pair` would wait out its whole
    // budget; the test has what it came for.
    dial.abort();

    assert!(
        still_pairing,
        "the dial ended the moment disconnect took the slot, so a pairing that might have \
         presented the code was abandoned"
    );
    assert!(
        events.events().is_empty(),
        "nothing connected, so nothing is announced: {:?}",
        events.events()
    );
}

/// A dial with **no code to spend**, the ordinary reconnect, has nothing to
/// lose, so `disconnect` ends it while it waits.
///
/// It matters because the gate has two jobs and only one of them is about the
/// code. A codeless dial that reached nobody would still install, and
/// `connect_watch` cannot tell "never reached" from "went away" — both are
/// `Idle` — so `gglib remote status` would say **Connected** for the ninety
/// seconds of that file's grace and then announce the loss of a connection
/// that never existed.
///
/// Found by an adversarial review of this PR: routing `code.is_none()` around
/// the gate left all 88 remote tests green, because every other call-site test
/// here dials with a code.
#[tokio::test(flavor = "multi_thread")]
async fn a_codeless_dial_waits_for_the_far_machine_too() {
    let (core, ops, events) = test_remote_ops().await;
    // A bare ticket is admitted only when the stored pairing names that same
    // machine, so the record has to name this one.
    core.settings()
        .update(paired_with(TICKET_UNREACHABLE, KEY_A))
        .await
        .expect("a pairing naming that machine admits a bare ticket for it");
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("a free loopback port")
        .local_addr()
        .expect("the port just bound")
        .port();

    let dialling = Arc::clone(&ops);
    let dial = tokio::spawn(async move {
        dialling
            .join(JoinRequest {
                pairing: Some(TICKET_UNREACHABLE.to_owned()),
                port: Some(port),
                discovery: false,
                ..JoinRequest::default()
            })
            .await
    });
    parked(port, &dial).await;

    tokio::time::timeout(PROMPTLY, ops.disconnect())
        .await
        .expect("disconnect queued behind the dial it exists to end")
        .expect("a dial that has reached nobody yet is one to give up on");

    let err = tokio::time::timeout(ENDED, dial)
        .await
        .expect("the dial ran on past the disconnect that ended it")
        .expect("the dial task panicked")
        .expect_err("nobody was reached, so nothing connected");
    assert!(
        matches!(err, GuiError::Conflict(_)),
        "a codeless dial that reached nobody installed anyway: {err:?}"
    );
    // The port goes with it — `dial`'s own comment says no arm may leave one
    // bound behind a failure it reported.
    //
    // **This does not pin `shutdown_timeout`, and saying so is the point.**
    // Deleting that call leaves this green: dropping the handle tears the
    // listener down too, just without waiting for it, and the drop wins the
    // race often enough that no assertion here can tell the two apart. What it
    // does pin is the arm existing at all. The difference the mutation makes is
    // visible only as wall clock — 4.5s pristine against 1.5s without — which
    // is a measurement, not a test. Recorded in the PR body as a surviving
    // mutation rather than papered over.
    assert!(
        TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err(),
        "the dial reported a failure and left its port bound and answering"
    );
    assert!(
        events.events().is_empty(),
        "nothing connected, so nothing is announced: {:?}",
        events.events()
    );
}

/// How long to watch a dial that should be parked in the gate.
///
/// **This is the assertion that discriminates, and it is a dwell rather than a
/// race.** The gate holds a dial for 25 or 30 seconds; a dial that skipped it is
/// over in microseconds — it binds, settles, installs and returns. Half a
/// second sits three orders of magnitude above the one and two below the
/// other, so it separates them without depending on which task the scheduler
/// happens to run first.
///
/// An earlier version of this file asserted only that `disconnect` won the
/// race to the slot, and an adversarial review showed that passes with the
/// gate skipped: the port becomes connectable *before* the install completes,
/// so `disconnect` still finds a reservation and the dial still ends in the
/// same `Conflict`. The dwell is what makes the two distinguishable.
const PARKED: Duration = Duration::from_millis(500);

/// Wait until the tunnel's local listener is accepting on `port`, then check
/// the dial is still parked rather than already over.
///
/// The port poll is the precise signal that `modelpipe::connect` has returned
/// and `dial` is into what follows it, which a bare sleep only approximates.
/// The dwell after it is the claim.
async fn parked<T>(port: u16, dial: &JoinHandle<T>) {
    tokio::time::timeout(PROMPTLY, async {
        while TcpStream::connect((Ipv4Addr::LOCALHOST, port))
            .await
            .is_err()
        {
            assert!(
                !dial.is_finished(),
                "the dial ended before it ever bound a port"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the dial never bound its local port");

    tokio::time::sleep(PARKED).await;
    assert!(
        !dial.is_finished(),
        "the dial ran to its end without ever waiting for the far machine — it bound a port, \
         used it, and installed a connection to a machine that had never answered"
    );
}
