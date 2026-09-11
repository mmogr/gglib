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

    ops.disable().await.expect("disable");
}
