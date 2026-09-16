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
/// unconditionally — so every daemon start opened a live two-minute code
/// nobody would ever read, on a ticket that no longer changes between
/// sessions.
/// The switch is a standing answer about reachability; it is not a person
/// asking to pair something.
#[tokio::test]
async fn a_resume_puts_the_tunnel_back_without_opening_a_pairing_window() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
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
        "a resume opens no pairing window; a code nobody is watching for is a live code nobody spends"
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
    let (_core, _proxy, _events, ops, _arming) = ops_with_key().await;

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
/// one on every run. Forgetting it here also exercises `RemoteOps::forget`
/// on its real path: the edge, the key file and the roster, all three.
///
/// **Everything fallible is asserted after the cleanup**, for that reason:
/// an assertion that fires between the mint and the forget takes the rest
/// of the test with it and leaves a live key in the checkout — the precise
/// outcome the cleanup exists to prevent, reached by the test failing,
/// which is the one moment it is least likely to be noticed.
#[tokio::test]
async fn an_enable_asked_to_invite_offers_a_code_for_a_new_device() {
    let (_core, _proxy, _events, ops, _arming) = ops_with_key().await;

    let request = EnableRequest {
        invite: true,
        ..offline()
    };
    let enabled = ops.enable(request).await.expect("enable");
    let armed_already_up = enabled.already_up;
    let offered = enabled.pairing.expect("an invite was asked for");

    // Gather.
    let active = ops.status().await.pairing_active;
    // A second invite on the tunnel that is already up, which is what
    // `enable --invite` has to do rather than answer "already enabled".
    let again = ops
        .enable(EnableRequest {
            invite: true,
            ..offline()
        })
        .await;

    // Clean up. Including whatever the second invite minted: it is supposed
    // to have been refused, but if the regression the assertion below guards
    // against ever lands, it minted a device of its own — and forgetting only
    // the first would leave exactly the row this test exists to keep out of
    // the checkout, on the one path that is there to catch it.
    if let Ok(Enabled {
        pairing: Some(second),
        ..
    }) = &again
    {
        let _ = ops.forget(&second.device).await;
    }
    // Gathered, not asserted: a panic here skips the rest of the cleanup.
    let held = ops.forget(&offered.device).await;
    let twice = ops.forget(&offered.device).await;
    let stopped = ops.disable().await;

    // Judge.
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
    assert!(active, "and it is redeemable");
    assert!(
        !armed_already_up,
        "this call armed the session, so every flag it sent took"
    );
    assert!(
        matches!(&again, Err(GuiError::Conflict(m)) if m.contains("already open")),
        "an invite while one is open is refused for that reason, not for being enabled: {again:?}"
    );
    assert!(held.expect("forget"), "the device this minted was held");
    assert!(
        !twice.expect("forget"),
        "and forgetting it twice is false rather than an error"
    );
    stopped.expect("disable");
}

/// An invite onto a session already running says it armed nothing.
///
/// Such a call's flags are ignored — the grants belong to the `enable` that
/// armed the session — so a surface must tell the two apart before saying
/// what it changed, and cannot infer it: only "asked for `/mcp`, told no" is
/// visible in the rest of the answer, and `enable --invite` with no flags is
/// both the commoner case and indistinguishable from a fresh arm without it.
#[tokio::test]
async fn an_invite_onto_a_running_session_reports_that_it_armed_nothing() {
    let (_core, _proxy, _events, ops, _arming) = ops_with_key().await;
    ops.enable(offline()).await.expect("the first enable arms");

    let second = ops.invite().await;

    // Clean up before judging: this minted a real key.
    if let Ok(Enabled {
        pairing: Some(p), ..
    }) = &second
    {
        let _ = ops.forget(&p.device).await;
    }
    let stopped = ops.disable().await;

    let second = second.expect("an invite onto a live session is offered");
    assert!(second.already_up, "it answered from the running session");
    assert!(second.pairing.is_some(), "and still offered a code");
    stopped.expect("disable");
}
