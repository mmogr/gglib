//! The gate between binding the port and using it, at its call site.
//!
//! `first_contact_tests.rs` drives the policy — what waiting for the far
//! machine decides, and that nothing is paid when it fails. This file
//! asserts the other half, which no test over a pure function can: that
//! `dial` actually goes through it, and goes through it *first*.
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

/// A dial that is still waiting for the far machine has spent nothing yet,
/// and `disconnect` is what ends it.
///
/// From modelpipe 0.3.0 `connect` returns as soon as the local port is
/// bound and dials behind the handle, so without a gate the very next thing
/// `dial` does — redeem the one-time code — goes out down a pipe that has
/// reached nobody, and is spent on the `502` the edge answers while there
/// is no peer. Minting another is a walk to the other machine.
///
/// It needs no network, for the reason `a_dial_that_fails_gives_the_connect_side_back`
/// needs none: the local bind is the first thing `modelpipe::connect` does,
/// and the iroh endpoint binds its own socket without anything answering.
/// That test makes the bind *fail* to reach `dial`; this one lets it
/// succeed, which is the only way to reach what `dial` does afterwards.
///
/// The wait is for the port to accept, not a sleep. A sleep would be a race
/// on a loaded machine — and a race that hides the regression rather than
/// reporting it, because the arm it would land in is the one that passes.
///
/// **Two mutations die here.** Delete the gate and the redeem goes out at
/// once, fails against the peerless edge, and the dial is over before
/// `disconnect` is called — so `disconnect` answers "not connected" and the
/// dial's error is a refusal rather than a cancellation. Move the gate to
/// *after* the redeem and the same thing happens for the same reason.
#[tokio::test(flavor = "multi_thread")]
async fn a_dial_still_waiting_for_the_far_machine_has_not_spent_the_code_yet() {
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
            .connect(ConnectRequest {
                // With a code: a first pairing is the case that has
                // something to spend, and the case this gate exists for.
                pairing: Some(format!("{TICKET_UNREACHABLE}-483920")),
                port: Some(port),
                discovery: false,
                ..ConnectRequest::default()
            })
            .await
    });
    parked(port, &dial).await;

    tokio::time::timeout(PROMPTLY, ops.disconnect())
        .await
        .expect("disconnect queued behind the dial it exists to end")
        .expect("a dial that has reached nobody yet is one to give up on");

    let err = tokio::time::timeout(PROMPTLY, dial)
        .await
        .expect("the dial ran on past the disconnect that ended it")
        .expect("the dial task panicked")
        .expect_err("nobody was reached, so nothing connected");
    let GuiError::Conflict(message) = err else {
        panic!(
            "the dial was still waiting for the far machine, so giving up on it is a conflict \
             — a redeem that had already gone out looks exactly like this: {err:?}"
        );
    };
    assert!(
        message.contains("cancelled by `gglib remote disconnect`"),
        "{message}"
    );
    assert!(
        events.events().is_empty(),
        "nothing connected, so nothing is announced: {:?}",
        events.events()
    );
}

/// The same claim for a dial with **no code to spend**, which is the ordinary
/// reconnect and the one the test above cannot make.
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
            .connect(ConnectRequest {
                pairing: Some(TICKET_UNREACHABLE.to_owned()),
                port: Some(port),
                discovery: false,
                ..ConnectRequest::default()
            })
            .await
    });
    parked(port, &dial).await;

    tokio::time::timeout(PROMPTLY, ops.disconnect())
        .await
        .expect("disconnect queued behind the dial it exists to end")
        .expect("a dial that has reached nobody yet is one to give up on");

    let err = tokio::time::timeout(PROMPTLY, dial)
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
/// race.** The gate holds a dial for thirty seconds; a dial that skipped it is
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
