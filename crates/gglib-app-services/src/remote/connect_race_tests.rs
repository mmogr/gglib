//! Tests for two connect-side commands arriving at once.
//!
//! Its own file rather than more of `connect_tests.rs`, which is at the
//! size budget, and because these are one subject: `connect` reserves the
//! connect side for the length of a dial, and every other command has to
//! stay answerable while it does.
//!
//! Three of the four need no network. Two put the slot into the state a
//! dial leaves it in and ask what the other commands do with it; the third
//! drives the real `connect` into `dial` by taking the loopback port it
//! would bind, because `modelpipe::connect` binds that listener before it
//! touches iroh. Without the third the other two would still pass with
//! every line between the reservation and the install deleted.

use std::net::Ipv4Addr;
use std::time::Duration;

use gglib_core::events::AppEvent;
use tokio::net::TcpListener;

use super::*;
use crate::test_support_remote::{
    KEY_A, TICKET_A, TICKET_UNREACHABLE, paired_with, test_remote_ops,
};

/// What `gglib remote status` allows the daemon before it gives up
/// (`gglib-cli/src/daemon_client/remote.rs`). A status that takes longer
/// than this is a status nobody sees.
const CLI_STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// `status` and `disconnect` answer while a dial holds the connect side.
///
/// Finding A1 without an iroh endpoint: the slot is put into the state a
/// dial leaves it in, which is the state that used to be a held mutex. Both
/// commands locked that mutex — `status` at the end of its snapshot,
/// `disconnect` to take the connection — and `tokio::sync::Mutex` is
/// FIFO-fair, so both waited out the dial. `remote status` gives the daemon
/// five seconds; `remote disconnect` is the command that exists to end a
/// dial and could not, because it queued behind it.
#[tokio::test]
async fn status_and_disconnect_do_not_wait_on_a_reserved_connect_side() {
    let (_, ops, events) = test_remote_ops().await;
    // What `connect` leaves in the slot for the length of its dial.
    let cancel = ops
        .live_connect
        .lock()
        .await
        .reserve(1)
        .expect("nothing holds the connect side");

    let status = tokio::time::timeout(CLI_STATUS_TIMEOUT, ops.status())
        .await
        .expect("status waited on the dial's slot past the timeout the CLI gives it");
    assert!(
        status.connected.is_none(),
        "a dial still in flight is not a connection to report"
    );

    tokio::time::timeout(CLI_STATUS_TIMEOUT, ops.disconnect())
        .await
        .expect("disconnect waited on the very dial it exists to cancel")
        .expect("a dial in flight is something to disconnect from");
    assert!(cancel.is_cancelled(), "the dial was not told to stop");
    assert!(
        !events
            .events()
            .iter()
            .any(|e| matches!(e, AppEvent::RemoteDisconnected)),
        "nothing was ever connected, so nothing is announced as gone: {:?}",
        events.events()
    );
}

/// A second `connect` during a dial is refused, and told which of the two
/// busy states it met.
///
/// "Already connected — disconnect first" would be the wrong sentence:
/// there is nothing connected yet, and the thing to do is wait rather than
/// tear anything down.
#[tokio::test]
async fn a_second_connect_while_one_is_dialling_says_a_dial_is_in_flight() {
    let (core, ops, _) = test_remote_ops().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing is stored");
    let _cancel = ops
        .live_connect
        .lock()
        .await
        .reserve(1)
        .expect("nothing holds the connect side");

    let err = ops
        .connect(ConnectRequest {
            pairing: Some(TICKET_A.to_owned()),
            ..ConnectRequest::default()
        })
        .await
        .expect_err("one dial at a time");
    let GuiError::Conflict(message) = err else {
        panic!("a dial already under way is a conflict: {err:?}");
    };
    assert!(message.contains("already dialling"), "{message}");
}

/// `connect` reserves the connect side, dials holding no lock, and gives
/// the reservation back when the dial fails.
///
/// The test that proves the two above are about a state production reaches.
/// They put the slot into `Filling` by hand, so on their own they say only
/// what `status` and `disconnect` do when they find it that way — every
/// line of `connect` between the reservation and the install could be
/// deleted and they would still pass. This one drives the real `connect`
/// through that whole span.
///
/// It gets there without a network because `modelpipe::connect` binds the
/// local listener on its first line, before it touches iroh at all: a port
/// already taken fails the dial there and nothing is dialled. So the
/// reservation is made, `dial` runs, and the failure path is exercised on
/// every host, IPv6 or not.
///
/// The timeout is the assertion that no lock is held across the dial, not
/// housekeeping. Giving the slot back re-locks the mutex `connect` would
/// have been holding, and `tokio::sync::Mutex` is not reentrant — a
/// `connect` that kept its guard for the dial deadlocks here rather than
/// returning the error.
#[tokio::test]
async fn a_dial_that_fails_gives_the_connect_side_back() {
    let (core, ops, events) = test_remote_ops().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing is stored");
    let occupied = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("a free loopback port");
    let port = occupied.local_addr().expect("the port just bound").port();

    let err = tokio::time::timeout(
        CLI_STATUS_TIMEOUT,
        ops.connect(ConnectRequest {
            pairing: Some(TICKET_A.to_owned()),
            port: Some(port),
            discovery: false,
            ..ConnectRequest::default()
        }),
    )
    .await
    .expect("connect held the connect side's lock across its own dial")
    .expect_err("that port is taken");
    let GuiError::Conflict(message) = err else {
        panic!("a port somebody else holds is a conflict: {err:?}");
    };
    assert!(message.contains(&port.to_string()), "{message}");

    assert!(
        ops.live_connect.lock().await.busy().is_none(),
        "a dial that failed kept the connect side, so nothing could ever dial again"
    );
    assert!(
        events.events().is_empty(),
        "nothing connected, so nothing is announced: {:?}",
        events.events()
    );
}

/// `status` answers while a *real* dial is in flight — finding A1, end to
/// end.
///
/// `connect` held `live_connect` from its first line to its last, across
/// `modelpipe::connect` and a redeem that may take twenty seconds, while
/// `status` locked the same mutex to read the connect snapshot. It now
/// checks and reserves under the lock, releases it across the dial, and
/// re-acquires to install — so this is the whole path, with a dial that
/// really is hanging.
///
/// Still `#[ignore]`d, and now for one reason rather than two. It binds a
/// real iroh endpoint against n0's public relays, the way `pidfile::sweep`'s
/// real-directory test touches the real `pids_dir()` — and it *fails*,
/// rather than passing vacuously, on a machine where the dial ends by
/// itself, which is what a runner with no IPv6 route gives. CI runs
/// `cargo test` without `--ignored`, so as an un-ignored test this would be
/// a coin toss on a correct tree. Run it by hand with
/// `cargo test -p gglib-app-services -- --ignored`.
///
/// Nothing rests on it. The three tests above make the same claim without a
/// network: two against the state a dial leaves behind, and
/// [`a_dial_that_fails_gives_the_connect_side_back`] through the real
/// `connect`, which reaches the dial by taking the port it would bind.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "binds a real iroh endpoint, and says nothing on a host with no IPv6 route"]
async fn status_answers_while_a_dial_is_in_flight() {
    let (core, ops, _) = test_remote_ops().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("a pairing naming that machine admits a bare ticket for it");

    let dialling = Arc::clone(&ops);
    let dial = tokio::spawn(async move {
        dialling
            .connect(ConnectRequest {
                pairing: Some(TICKET_UNREACHABLE.to_owned()),
                discovery: false,
                ..ConnectRequest::default()
            })
            .await
    });
    // Long enough for the spawned task to have taken the lock, short enough
    // that the assertion below is still about the dial and not about it.
    tokio::time::sleep(Duration::from_millis(250)).await;

    let answered = tokio::time::timeout(CLI_STATUS_TIMEOUT, ops.status()).await;
    // A dial that gave up on its own released the lock, and then a fast
    // `status` proves nothing. Checked before the verdict so this cannot go
    // green on a machine where the address fails immediately for want of
    // any IPv6 route at all.
    let still_dialling = !dial.is_finished();
    dial.abort();
    assert!(
        still_dialling,
        "the dial ended by itself, so this run said nothing about the lock"
    );
    assert!(
        answered.is_ok(),
        "status waited on the dial's mutex past the timeout the CLI gives it"
    );
}
