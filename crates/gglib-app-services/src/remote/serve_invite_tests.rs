//! Tests for what arming the tunnel *offers*: nothing, unless someone asked
//! to pair a device.
//!
//! Split from `serve_watch_tests.rs`, which is at its size budget and is
//! about a different subject — what the serve side does about the proxy it
//! fronts. These share that file's fixture, and through it `enable_tests.rs`'s:
//! a real proxy on a free port, and a `modelpipe::serve` that binds an
//! endpoint without reaching the network, so the whole of `arm` runs here
//! rather than stopping at the bind.

use gglib_core::SettingsUpdate;

use super::serve_watch_tests::{offline, ops_with_key};
use super::*;
use crate::error::GuiError;

/// A restart puts the tunnel back and arms no pairing code.
///
/// `resume` used to reach the tunnel through `enable`, which mints a code
/// unconditionally — so every daemon start opened a live two-minute grant
/// for a code nobody would ever read, on a ticket that no longer changes
/// between sessions and a route that sits outside the proxy's bearer group.
/// The switch is a standing answer about reachability; it is not a person
/// asking to pair something.
#[tokio::test]
async fn a_resume_puts_the_tunnel_back_without_opening_a_pairing_window() {
    let (core, _proxy, _events, ops) = ops_with_key().await;
    // The state `enable` leaves behind: the switch on, and the flags it was
    // given, which is what `resume` arms from.
    core.settings()
        .update(SettingsUpdate {
            remote_enabled: Some(Some(true)),
            remote_serve: Some(Some(gglib_core::RemoteServe {
                allow_mcp: false,
                relay: None,
                discovery: false,
            })),
            ..SettingsUpdate::default()
        })
        .await
        .expect("settings update");

    ops.resume().await;

    let status = ops.status().await;
    assert!(
        status.enabled,
        "the tunnel is up: a resume that armed nothing would make this vacuous"
    );
    assert!(
        !status.pairing_active,
        "a resume opens no pairing window; a code nobody is watching for is a live grant nobody spends"
    );

    ops.disable().await.expect("disable");
}

/// A plain `enable` is a switch, and switches do not hand out credentials.
///
/// The device a person is pairing is named by `invite`, so the ordinary
/// `enable` — and every resume, which goes through the same path — arms the
/// tunnel and nothing else.
#[tokio::test]
async fn a_plain_enable_brings_the_tunnel_up_and_offers_nothing() {
    let (_core, _proxy, _events, ops) = ops_with_key().await;

    let enabled = ops.enable(offline()).await.expect("enable");

    assert!(enabled.pairing.is_none(), "a switch offers no pairing code");
    assert!(!enabled.ticket.is_empty(), "but the tunnel is up and named");
    assert!(
        !ops.status().await.pairing_active,
        "and nothing is redeemable"
    );

    ops.disable().await.expect("disable");
}

/// The contrast, so the tests above cannot pass by arming nothing at all:
/// asked for an invite, `enable` mints a device key and a code for it.
///
/// **This one forgets what it minted, and the cleanup is not politeness.**
/// `invite` writes a real key into `<data root>/data/remote_devices`, which a
/// debug build resolves to the repository checkout — the same file a
/// developer's own daemon seeds its listener from. A test that left rows
/// behind would arm that machine's tunnel with ids nobody issued, growing by
/// one on every run. Forgetting them here is also the only coverage
/// `RemoteOps::forget` has of its real path: the edge, the key file and the
/// roster, all three.
#[tokio::test]
async fn an_enable_asked_to_invite_offers_a_code_for_a_new_device() {
    let (_core, _proxy, _events, ops) = ops_with_key().await;

    let request = EnableRequest {
        invite: true,
        ..offline()
    };
    let enabled = ops.enable(request).await.expect("enable");
    let offered = enabled.pairing.expect("an invite was asked for");

    assert_eq!(
        offered.code.len(),
        6,
        "six digits, as ADR 0012 decision 3 has it"
    );
    assert!(
        offered.pairing.ends_with(&offered.code),
        "the pairing string carries the code"
    );
    assert!(
        offered.device.starts_with("dev-"),
        "and names the device the key was minted for: {}",
        offered.device
    );
    assert!(ops.status().await.pairing_active, "and it is redeemable");

    // A second invite on the tunnel that is already up, which is what
    // `enable --invite` has to do rather than answer "already enabled".
    let again = ops
        .enable(EnableRequest {
            invite: true,
            ..offline()
        })
        .await;
    assert!(
        matches!(&again, Err(GuiError::Conflict(m)) if m.contains("already open")),
        "an invite while one is open is refused for that reason, not for being enabled: {again:?}"
    );

    assert!(
        ops.forget(&offered.device).await.expect("forget"),
        "the device this minted was held"
    );
    assert!(
        !ops.forget(&offered.device).await.expect("forget"),
        "and forgetting it twice is false rather than an error"
    );

    ops.disable().await.expect("disable");
}

/// The row a device redeemed is stamped, which is what tells it from an
/// invite nobody took.
///
/// `joined_at` is when the code was *minted* — the row and the key are
/// written before it is ever shown, so a device cannot end up holding a key
/// this side has no record of — and it therefore says nothing about whether
/// anybody arrived. Without a second timestamp, a device that paired a
/// minute ago and has not yet made a request is indistinguishable from a
/// two-minute window that expired unwatched, and `list` would have to call
/// them the same thing.
///
/// **Everything fallible is asserted after the cleanup**, not before. This
/// test mints a real key into the checkout, and an assertion is a panic: one
/// failing between the mint and the `forget` leaves a live device id behind
/// that a developer's own `enable` would seed onto their tunnel. Gathering
/// first and judging afterwards costs a few locals and means a red run
/// cleans up exactly as a green one does.
#[tokio::test]
async fn redeeming_an_invite_stamps_the_row_it_was_minted_for() {
    let (_core, _proxy, _events, ops) = ops_with_key().await;

    let enabled = ops
        .enable(EnableRequest {
            invite: true,
            ..offline()
        })
        .await
        .expect("enable");
    let offered = enabled.pairing.expect("an invite was asked for");

    let minted = row(&ops, &offered.device).await;
    let outcome = gglib_core::ports::RemoteGatewayPort::redeem_pairing_code(
        &*ops.gateway,
        &offered.code,
        None,
        Some("Matt's iPhone"),
    );
    // `roster_sync` owns the write and the note reaches it down a channel,
    // so the stamp lands a moment after the redemption rather than with it.
    let joined = settled(&ops, &offered.device, |d| d.redeemed_at.is_some()).await;

    let forgotten = ops.forget(&offered.device).await.expect("forget");
    ops.disable().await.expect("disable");

    assert!(forgotten, "the device this minted was held");
    let minted = minted.expect("the invite wrote a roster row");
    assert!(minted.joined_at > 0, "the mint is stamped");
    assert!(
        minted.redeemed_at.is_none(),
        "and nobody has taken it: that is the whole distinction"
    );
    assert!(
        matches!(outcome, gglib_core::ports::PairingOutcome::Granted { .. }),
        "the code this test just minted is the right one: {outcome:?}"
    );
    let joined = joined.expect("the roster never settled within two seconds");
    assert_eq!(
        joined.label.as_deref(),
        Some("Matt's iPhone"),
        "the label rides the same write"
    );
    assert!(
        joined.last_seen.is_none(),
        "and no request has arrived under its key yet — which is the case a \
         row must not be called never-joined for"
    );
}

/// The row `list` holds for `device`, if it holds one.
async fn row(ops: &RemoteOps, device: &str) -> Option<DeviceView> {
    ops.list()
        .await
        .expect("list")
        .into_iter()
        .find(|d| d.id == device)
}

/// Poll [`row`] until `done`, or give up rather than hang.
///
/// The roster is written by a task, so there is no handle to await. A
/// deadline says how long "a moment" is allowed to be instead of leaving a
/// green run to depend on a scheduler. Returns `None` on giving up rather
/// than panicking, so the caller can clean up before it judges.
async fn settled(
    ops: &RemoteOps,
    device: &str,
    done: impl Fn(&DeviceView) -> bool,
) -> Option<DeviceView> {
    for _ in 0..100 {
        if let Some(seen) = row(ops, device).await
            && done(&seen)
        {
            return Some(seen);
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    None
}
