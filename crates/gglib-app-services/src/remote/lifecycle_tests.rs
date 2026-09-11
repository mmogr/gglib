//! Tests for [`super::RemoteOps`]'s serve side — what it reports and what
//! it refuses.
//!
//! `enable` is deliberately absent: it starts the proxy, which on this
//! fixture means binding the real proxy port, and it binds an iroh endpoint
//! on top. Everything it sits on is here, and the two-machine run in ADR
//! 0012 is what covers the rest.

use std::time::Duration;

use gglib_core::events::AppEvent;

use super::pairing::PAIRING_TTL;
use super::types::EnableRequest;
use crate::error::GuiError;
use crate::test_support_remote::{FINGERPRINT_A, KEY_A, TICKET_A, paired_with, test_remote_ops};

/// A daemon that has done nothing remote reports nothing remote. The tunnel
/// is off by default (ADR 0012) and the switch that would bring it back is
/// unset here, so a fresh process is this and only this.
#[tokio::test]
async fn a_daemon_that_has_done_nothing_remote_reports_every_side_as_off() {
    let (_, ops, _) = test_remote_ops().await;

    let status = ops.status().await;
    assert!(!status.enabled);
    assert!(!status.pairing_active);
    assert!(!status.paired);
    assert!(!status.mcp_allowed);
    assert_eq!(status.ticket_fingerprint, None);
    assert_eq!(status.path, None);
    assert!(status.peers.is_empty());
    assert!(status.connected.is_none());
    assert_eq!(status.stored_ticket_fingerprint, None);
    assert!(!status.has_remote_key);
    assert_eq!(status.tunnelled_requests, 0);
    assert_eq!(status.last_tunnelled_ms, None);
    assert_eq!(status.last_peer, None);
}

/// What settings remember of an earlier pairing reaches the status surface
/// as a fingerprint and a yes-or-no, and never as the ticket or the key.
///
/// Both are credentials in the sense that matters: any local client can
/// read the status route, and a ticket plus a key is the whole of what it
/// takes to be the far machine's client.
#[tokio::test]
async fn a_stored_pairing_is_reported_as_a_fingerprint_and_never_as_the_ticket_or_the_key() {
    let (core, ops, _) = test_remote_ops().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("an earlier pairing is stored");

    let status = ops.status().await;
    assert_eq!(
        status.stored_ticket_fingerprint.as_deref(),
        Some(FINGERPRINT_A)
    );
    assert!(status.has_remote_key);

    let rendered = format!("{status:?}");
    assert!(!rendered.contains(TICKET_A), "{rendered}");
    assert!(!rendered.contains(KEY_A), "{rendered}");
}

/// A stored ticket that no longer parses is reported as no ticket rather
/// than as an error, and the key bound to it is still reported as held.
///
/// The two answers come off one record but do not stand or fall together: a
/// ticket written by a newer build, or a format bumped past v0, must not
/// make `remote status` fail — it is the command someone runs *because*
/// something is wrong. What is lost is only the machine's name, which is
/// what a re-pair costs anyway.
#[tokio::test]
async fn a_stored_ticket_that_no_longer_parses_costs_the_fingerprint_and_nothing_else() {
    let (core, ops, _) = test_remote_ops().await;
    core.settings()
        .update(paired_with("pipe-from-some-later-format", KEY_A))
        .await
        .expect("a ticket this build cannot read is stored");

    let status = ops.status().await;
    assert_eq!(status.stored_ticket_fingerprint, None);
    assert!(
        status.has_remote_key,
        "a key is still held; what is lost is the name of the machine it is for"
    );
}

/// Disabling a tunnel that is not up is a conflict, and — the part worth
/// pinning — it announces nothing. `disable` also resets the gateway's
/// session, so an arm that ran on the refusal path would clear a pairing
/// belonging to a session that is still live.
#[tokio::test]
async fn disabling_a_tunnel_that_is_not_up_is_a_conflict_that_clears_nothing() {
    let (_, ops, events) = test_remote_ops().await;
    ops.gateway().pairing.begin_for(
        "483920".to_owned(),
        "sk-zzq-armed".to_owned(),
        "dev-0a1b2c3d".to_owned(),
        PAIRING_TTL,
    );

    let err = ops.disable().await.expect_err("nothing is enabled");
    assert!(matches!(err, GuiError::Conflict(_)), "{err:?}");
    assert!(
        ops.gateway().pairing.active(),
        "a refused disable is not a teardown"
    );
    assert!(
        !events
            .events()
            .iter()
            .any(|e| matches!(e, AppEvent::RemoteDisabled)),
        "{:?}",
        events.events()
    );
}

/// What `gglib remote status` allows the daemon before it gives up
/// (`gglib-cli/src/daemon_client/remote.rs`). A status that takes longer
/// than this is a status nobody sees.
const CLI_STATUS_TIMEOUT: Duration = Duration::from_secs(5);

/// `status` and `disable` answer while an `enable` is still arming.
///
/// `enable` held the serve slot's mutex across a five-second settings-cache
/// window and a ten-second wait for a relay, and `status` locks the same
/// mutex to read the serve side. Fifteen seconds is three times what the
/// CLI gives status, so the command someone runs to find out whether the
/// ticket is ready was the one command that could not answer while it was
/// being made.
///
/// The slot is put into the state arming leaves it in, rather than reached
/// through `enable` — that would bind the proxy's port for real on this
/// fixture, and an iroh endpoint on top of it.
#[tokio::test]
async fn status_and_disable_do_not_wait_on_a_serve_side_that_is_still_arming() {
    let (_, ops, events) = test_remote_ops().await;
    let cancel = ops
        .live
        .lock()
        .await
        .reserve(1)
        .expect("nothing holds the serve side");

    let status = tokio::time::timeout(CLI_STATUS_TIMEOUT, ops.status())
        .await
        .expect("status waited on the arming past the timeout the CLI gives it");
    assert!(
        !status.enabled,
        "a tunnel that is still binding is not one to report as enabled"
    );

    tokio::time::timeout(CLI_STATUS_TIMEOUT, ops.disable())
        .await
        .expect("disable waited on the arming it exists to give up on")
        .expect("an arming is something to disable");
    assert!(cancel.is_cancelled(), "the arming was not told to stop");
    assert!(
        !events
            .events()
            .iter()
            .any(|e| matches!(e, AppEvent::RemoteDisabled)),
        "no ticket was ever handed out, so nothing is announced as gone: {:?}",
        events.events()
    );
}

/// A second `enable` during an arming is refused, and told which of the two
/// busy states it met — waiting is the thing to do, not disabling.
#[tokio::test]
async fn a_second_enable_while_one_is_arming_says_a_ticket_is_on_its_way() {
    let (_, ops, _) = test_remote_ops().await;
    let _cancel = ops
        .live
        .lock()
        .await
        .reserve(1)
        .expect("nothing holds the serve side");

    let err = ops
        .enable(EnableRequest::default())
        .await
        .expect_err("one arming at a time");
    let GuiError::Conflict(message) = err else {
        panic!("an arming already under way is a conflict: {err:?}");
    };
    assert!(message.contains("already being enabled"), "{message}");
}
