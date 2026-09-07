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
use crate::test_support_remote::{TICKET_UNREACHABLE, test_remote_ops};

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
    waiting(port, &dial).await;

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

/// Wait until the tunnel's local listener is accepting on `port` and the
/// dial is still going.
///
/// The precise signal that `modelpipe::connect` has returned and `dial` is
/// in what follows it, which a sleep only approximates.
///
/// **The `is_finished` check is the assertion, not a guard.** A dial that
/// reached its end while this was still watching for the port is a dial
/// that never waited for anything — it bound, redeemed into the peerless
/// edge, failed, and tore the port down again, all inside one poll. Without
/// this the same regression arrives as "the port never opened", which is
/// both wrong and the hardest sentence to act on.
async fn waiting<T>(port: u16, dial: &JoinHandle<T>) {
    tokio::time::timeout(PROMPTLY, async {
        loop {
            assert!(
                !dial.is_finished(),
                "the dial ran to its end without ever waiting for the far machine, so the \
                 one-time code went out down a pipe that had reached nobody"
            );
            if TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the dial never bound its local port");
}
