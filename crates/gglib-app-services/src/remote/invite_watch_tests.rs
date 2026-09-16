//! What a device redeeming its invite at the tunnel edge leaves behind, over
//! a real pipe.
//!
//! These share `serve_watch_tests.rs`'s fixture: a real proxy on a free port,
//! and a `modelpipe::serve` that binds an endpoint without reaching the
//! network. The device dials that endpoint with `modelpipe::pair`, as a laptop
//! or a phone does, with discovery off, so the pairing never leaves this
//! machine.

use std::time::Duration;

use super::super::RemoteOps;
use super::super::serve_watch_tests::{offline, ops_with_key};
use super::super::types::{DeviceView, EnableRequest};

/// The row `list` holds for `device` once `done` says it has settled, or
/// `None` after two seconds.
///
/// The roster is written by a task, so there is no handle to await. A
/// deadline says how long "a moment" is allowed to be, and `None` rather than
/// a panic lets the caller clean up before it judges.
async fn settled(
    ops: &RemoteOps,
    device: &str,
    done: impl Fn(&DeviceView) -> bool,
) -> Option<DeviceView> {
    for _ in 0..100 {
        let seen = ops
            .list()
            .await
            .expect("list")
            .into_iter()
            .find(|d| d.id == device);
        if let Some(seen) = seen
            && done(&seen)
        {
            return Some(seen);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    None
}

/// A device that redeems its code is recorded: when, what it called itself,
/// and the endpoint it paired from, which `gglib remote list` shows.
///
/// **Everything fallible is asserted after the cleanup**, as in
/// `serve_invite_tests.rs`: this mints a real key into the key file, and a
/// panic between the mint and the `forget` would leave it there.
#[tokio::test(flavor = "multi_thread")]
async fn a_device_that_redeems_its_invite_is_recorded_with_the_endpoint_it_paired_from() {
    let (_core, _proxy, _events, ops, _arming) = ops_with_key().await;
    let enabled = ops
        .enable(EnableRequest {
            invite: true,
            ..offline()
        })
        .await
        .expect("enable");
    let offered = enabled.pairing.expect("an invite was asked for");
    let before = ops
        .list()
        .await
        .map(|rows| rows.into_iter().find(|d| d.id == offered.device));

    // Gather.
    let mut opts = modelpipe::ConnectOptions::default();
    opts.discovery = false;
    opts.port_mapping = false;
    let paired = match offered.pairing.parse::<modelpipe::PairingString>() {
        Ok(pairing) => modelpipe::pair(
            &pairing,
            Some("Matt's iPhone"),
            opts,
            Duration::from_secs(20),
        )
        .await
        .map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    let joined = settled(&ops, &offered.device, |d| d.redeemed_at.is_some()).await;
    let from = paired
        .as_ref()
        .ok()
        .map(|paired| paired.handle.peer_id().fingerprint());
    let still_open = ops.status().await.pairing_active;

    // Clean up.
    if let Ok(paired) = &paired {
        paired.handle.shutdown_timeout(Duration::ZERO).await;
    }
    let forgotten = ops.forget(&offered.device).await;
    let stopped = ops.disable().await;

    // Judge.
    let paired = paired.expect("the device pairs over the pipe");
    assert_eq!(
        paired.device, offered.device,
        "and is handed the key minted for that row"
    );
    let before = before
        .expect("list")
        .expect("the invite's row is listed before anyone redeems it");
    assert!(
        before.joined_at > 0,
        "the row says when the invite was minted"
    );
    assert_eq!(before.redeemed_at, None, "and that nobody has redeemed it");
    let joined = joined.expect("the roster never recorded the redemption within two seconds");
    assert_eq!(joined.label.as_deref(), Some("Matt's iPhone"));
    assert_eq!(
        joined.last_seen, None,
        "pairing is not a request under the device's key"
    );
    assert_eq!(
        joined.peer, from,
        "the row names the endpoint the device paired from"
    );
    assert!(!still_open, "and the code is spent");
    assert!(
        forgotten.expect("forget"),
        "the device this minted was held"
    );
    stopped.expect("disable");
}
