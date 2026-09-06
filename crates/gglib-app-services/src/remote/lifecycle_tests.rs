//! Tests for [`super::RemoteOps`]'s serve side — what it reports and what
//! it refuses.
//!
//! `enable` is deliberately absent: it starts the proxy, which on this
//! fixture means binding the real proxy port, and it binds an iroh endpoint
//! on top. Everything it sits on is here, and the two-machine run in ADR
//! 0012 is what covers the rest.

use gglib_core::events::AppEvent;

use super::*;
use crate::test_support_remote::{FINGERPRINT_A, KEY_A, TICKET_A, paired_with, test_remote_ops};

/// A daemon that has done nothing remote reports nothing remote. The
/// tunnel is off by default and never persisted (ADR 0012), so a fresh
/// process is this and only this.
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
    ops.gateway()
        .pairing
        .begin("483920".to_owned(), "sk-zzq-armed".to_owned(), PAIRING_TTL);

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
