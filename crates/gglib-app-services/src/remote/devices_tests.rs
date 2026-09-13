//! Tests for what `invite` says when there is no tunnel to invite onto, and
//! for the order `forget` does its work in.
//!
//! A `#[path]` sibling of `devices.rs` rather than more of the serve side's
//! own test files, which are at their size budget.

use futures_util::FutureExt as _;
use gglib_core::SettingsUpdate;
use gglib_core::ports::{PairingOutcome, RemoteGatewayPort};

use super::super::pairing::PAIRING_TTL;
use super::*;
use crate::test_support_remote::test_remote_ops;

/// The refusal an `invite` got, or a panic naming what came back instead.
fn refusal(outcome: Result<Enabled, GuiError>) -> String {
    match outcome {
        Err(GuiError::Conflict(message)) => message,
        other => panic!("an invite with no tunnel up is a conflict: {other:?}"),
    }
}

/// With the switch off, `enable` is the fix, and the refusal names it. With a
/// tunnel still arming, which is another command's `enable` or a resume that
/// outlasted the wait `invite` gives it, a second `enable` refuses while the
/// slot is held, so naming it would send someone to a refusal: the refusal
/// says to wait instead, and how to give up.
///
/// The slot is put into the state arming leaves it in, rather than reached
/// through `enable`, which would bind the proxy's port for real.
#[tokio::test]
async fn an_invite_while_the_tunnel_is_arming_says_to_wait_for_it() {
    let (_, ops, _) = test_remote_ops().await;
    let off = refusal(ops.invite().await);

    let _cancel = ops
        .live
        .lock()
        .await
        .reserve(1)
        .expect("nothing holds the serve side");
    let arming = refusal(ops.invite().await);

    assert!(off.contains("`gglib remote enable`"), "{off}");
    assert!(arming.contains("still coming up"), "{arming}");
    assert!(arming.contains("`gglib remote status`"), "{arming}");
    assert!(arming.contains("`gglib remote invite`"), "{arming}");
    assert!(
        arming.contains("`gglib remote disable`"),
        "the way out, as a second enable's refusal gives it: {arming}"
    );
    assert!(
        !arming.contains("`gglib remote enable"),
        "a second enable refuses while another is arming: {arming}"
    );
}

/// The switch on and the slot empty, with no resume working: the resume that
/// puts the tunnel back has given up, or had not begun when this looked.
/// `enable --invite` is the fix either way. It arms the tunnel again with a
/// code, and waits for a resume that has only just begun. It was the wrong
/// advice while an `enable` meeting the resume was refused (#1037).
#[tokio::test]
async fn an_invite_to_a_daemon_switched_on_with_nothing_arming_says_enable_invite_puts_it_back() {
    let (core, ops, _) = test_remote_ops().await;
    core.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            ..SettingsUpdate::default()
        })
        .await
        .expect("the switch is recorded");

    let message = refusal(ops.invite().await);
    assert!(message.contains("switched on"), "{message}");
    assert!(
        message.contains("`gglib remote enable --invite`"),
        "the command that puts it back, with a code: {message}"
    );
}

/// `forget` withdraws a device's open invite before it waits on anything.
///
/// Withdrawn last, the code stayed redeemable across the roster lock and two
/// store writes, for a key the edge had already dropped: a device that pairs,
/// is shown a checkmark, and is refused on its first real request. The roster
/// lock is held here, so `forget` parks on it; one poll reaches that point,
/// and the redemption made while it waits is the one that must fail.
///
/// The id is outside the shape this machine mints on purpose. This goes
/// through the real `forget`, which reads the process-wide key file, and an
/// id that could never be minted can never be in it — so nothing is written,
/// and no lock against a real minting is needed.
#[tokio::test]
async fn forgetting_a_device_withdraws_its_open_invite_before_waiting_on_the_stores() {
    let (_, ops, _) = test_remote_ops().await;
    ops.gateway().pairing.begin_for(
        "483920".to_owned(),
        "sk-zzq-armed".to_owned(),
        "dev-never-minted".to_owned(),
        PAIRING_TTL,
    );

    let held = ops.roster.lock().await;
    let mut forgetting = std::pin::pin!(ops.forget("dev-never-minted"));
    let parked = forgetting.as_mut().now_or_never().is_none();
    let outcome = RemoteGatewayPort::redeem_pairing_code(&*ops.gateway(), "483920", None, None);
    drop(held);
    let forgotten = forgetting.await;

    assert!(parked, "forget waits on the roster lock this test holds");
    assert!(
        matches!(outcome, PairingOutcome::Rejected),
        "a code for a device being forgotten redeemed while forget waited: {outcome:?}"
    );
    assert!(forgotten.is_ok(), "{forgotten:?}");
}
