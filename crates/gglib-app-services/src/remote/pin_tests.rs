//! A device's key admits only from the endpoint that redeemed its invite,
//! over a real pipe.
//!
//! The fixture is `invite_watch_tests.rs`'s: a real proxy, an offline serve,
//! and a device that pairs with `modelpipe::pair`, discovery off. A device
//! keeps its endpoint in a file in the test's own scratch directory, as one
//! that stays admitted across a re-arm must. Everything fallible is judged
//! after the cleanup, for the reason that file gives.

use std::path::Path;
use std::time::Duration;

use gglib_core::Device;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use super::super::RemoteOps;
use super::super::device_keys::{read_keys, write_keys};
use super::super::roster::{read_roster, write_roster};
use super::super::serve_watch_tests::{offline, ops_with_key};
use super::super::types::{EnableRequest, OfferedPairing};
use super::invite_watch_tests::settled;

/// What the edge answers a key it does not admit from this endpoint.
pub(super) const REFUSED: u16 = 401;

/// What the proxy behind it answers a request the edge let through.
const ADMITTED: u16 = 200;

fn device_options(identity: Option<&Path>) -> modelpipe::ConnectOptions {
    let mut opts = modelpipe::ConnectOptions::default();
    opts.discovery = false;
    opts.port_mapping = false;
    opts.identity = identity.map(Path::to_path_buf);
    opts
}

/// Enable with an invite, and hand back the ticket and the code.
pub(super) async fn invited(ops: &RemoteOps) -> (String, OfferedPairing) {
    let enabled = ops
        .enable(EnableRequest {
            invite: true,
            ..offline()
        })
        .await
        .expect("enable");
    (
        enabled.ticket,
        enabled.pairing.expect("an invite was asked for"),
    )
}

/// Redeem `offered` as a device keeping its endpoint in `identity`.
pub(super) async fn pair(
    offered: &OfferedPairing,
    identity: &Path,
) -> Result<modelpipe::Paired, String> {
    let pairing = offered
        .pairing
        .parse::<modelpipe::PairingString>()
        .map_err(|e| e.to_string())?;
    let opts = device_options(Some(identity));
    modelpipe::pair(&pairing, Some("a laptop"), opts, Duration::from_secs(20))
        .await
        .map_err(|e| e.to_string())
}

/// Dial `ticket` as the endpoint in `identity`, or as a fresh one, and wait
/// for the far side.
async fn dial(ticket: &str, identity: Option<&Path>) -> Option<modelpipe::ConnectHandle> {
    let ticket = ticket.parse::<modelpipe::Ticket>().ok()?;
    let pipe = modelpipe::connect(&ticket, device_options(identity))
        .await
        .ok()?;
    if pipe.wait_reachable(Duration::from_secs(20)).await.is_err() {
        pipe.shutdown_timeout(Duration::ZERO).await;
        return None;
    }
    Some(pipe)
}

/// The status a request presenting `key` gets through `pipe`, or `None`
/// when it gets no answer within ten seconds.
pub(super) async fn status(pipe: &modelpipe::ConnectHandle, key: &str) -> Option<u16> {
    let ask = async {
        let mut stream = TcpStream::connect(pipe.local_addr()).await.ok()?;
        let request = format!(
            "GET /v1/models HTTP/1.1\r\nHost: gglib\r\nAuthorization: Bearer {key}\r\n\
             Connection: close\r\n\r\n"
        );
        stream.write_all(request.as_bytes()).await.ok()?;
        let mut head = [0u8; 12];
        stream.read_exact(&mut head).await.ok()?;
        std::str::from_utf8(&head[9..]).ok()?.parse().ok()
    };
    tokio::time::timeout(Duration::from_secs(10), ask)
        .await
        .ok()
        .flatten()
}

/// The status `key` gets from a fresh endpoint dialling `ticket`.
async fn status_from_a_copy(ticket: &str, key: &str) -> Option<u16> {
    let pipe = dial(ticket, None).await?;
    let answered = status(&pipe, key).await;
    pipe.shutdown_timeout(Duration::ZERO).await;
    answered
}

/// Once pairing has settled, the key admits from the endpoint that paired,
/// and the same key from any other endpoint is refused as a wrong one is.
#[tokio::test(flavor = "multi_thread")]
async fn a_paired_key_admits_only_from_the_endpoint_that_paired() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let (ticket, offered) = invited(&ops).await;

    let paired = pair(&offered, &scratch.path().join("laptop")).await;
    // Written after the swap, by the same task, so the pin is in place.
    let joined = settled(&ops, &offered.device, |d| d.redeemed_at.is_some())
        .await
        .is_some();
    let (own, copy) = match &paired {
        Ok(p) => (
            status(&p.handle, &p.api_key).await,
            status_from_a_copy(&ticket, &p.api_key).await,
        ),
        Err(_) => (None, None),
    };
    let row = read_roster(&core)
        .await
        .map(|rows| rows.into_iter().find(|d| d.id == offered.device));

    if let Ok(p) = &paired {
        p.handle.shutdown_timeout(Duration::ZERO).await;
    }
    let forgotten = ops.forget(&offered.device).await;
    let stopped = ops.disable().await;

    let paired = paired.expect("the device pairs over the pipe");
    assert!(
        joined,
        "the roster never recorded the redemption within two seconds"
    );
    assert_eq!(own, Some(ADMITTED), "admitted from its own endpoint");
    assert_eq!(copy, Some(REFUSED), "and refused from another");
    assert_eq!(
        row.expect("roster").expect("the row").endpoint,
        Some(paired.handle.peer_id().to_string()),
        "the row keeps the whole id the key is pinned to"
    );
    assert!(forgotten.expect("forget"));
    stopped.expect("disable");
}

/// A re-arm, which is what a restart runs, seeds the key pinned: the device
/// dialling back from its kept endpoint is admitted and a copy is not.
#[tokio::test(flavor = "multi_thread")]
async fn a_rearm_keeps_the_pin() {
    let (_core, _proxy, _events, ops, _arming) = ops_with_key().await;
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let identity = scratch.path().join("laptop");
    let (_, offered) = invited(&ops).await;

    let paired = pair(&offered, &identity).await;
    let joined = settled(&ops, &offered.device, |d| d.redeemed_at.is_some())
        .await
        .is_some();
    if let Ok(p) = &paired {
        p.handle.shutdown_timeout(Duration::ZERO).await;
    }
    let restarted = match ops.disable().await {
        Ok(()) => ops.enable(offline()).await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    let (back, copy) = match (&paired, &restarted) {
        (Ok(p), Ok(up)) => {
            let back = match dial(&up.ticket, Some(&identity)).await {
                Some(pipe) => {
                    let answered = status(&pipe, &p.api_key).await;
                    pipe.shutdown_timeout(Duration::ZERO).await;
                    answered
                }
                None => None,
            };
            (back, status_from_a_copy(&up.ticket, &p.api_key).await)
        }
        _ => (None, None),
    };

    let forgotten = ops.forget(&offered.device).await;
    let stopped = ops.disable().await;

    paired.expect("the device pairs over the pipe");
    assert!(
        joined,
        "the roster never recorded the redemption within two seconds"
    );
    restarted.expect("disable, then enable again");
    assert_eq!(back, Some(ADMITTED), "the device is admitted");
    assert_eq!(copy, Some(REFUSED), "and a copy is still refused");
    assert!(forgotten.expect("forget"));
    stopped.expect("disable");
}

/// A row with no endpoint, which is every device paired before keys were
/// pinned, is not admitted after an arm, and says to pair it again.
#[tokio::test(flavor = "multi_thread")]
async fn a_row_with_no_endpoint_is_not_admitted_after_an_arm() {
    let (core, _proxy, _events, ops, _arming) = ops_with_key().await;
    let (id, key) = ("dev-0a1b2c3d", "sk-zzq-unpinned");
    let written = async {
        let mut keys = read_keys(&ops)?;
        keys.insert(id.to_owned(), key.to_owned());
        write_keys(&ops, &keys)?;
        let mut roster = read_roster(&core).await?;
        roster.push(Device {
            id: id.to_owned(),
            label: None,
            joined_at: 1,
            redeemed_at: Some(2),
            last_seen: None,
            peer: Some("3ca82708b995".to_owned()),
            endpoint: None,
        });
        write_roster(&core, roster).await
    }
    .await;

    let enabled = ops.enable(offline()).await;
    let listed = ops.list().await;
    let copy = match &enabled {
        Ok(up) => status_from_a_copy(&up.ticket, key).await,
        Err(_) => None,
    };

    let forgotten = ops.forget(id).await;
    let stopped = ops.disable().await;

    written.expect("the old row and its key are stored");
    enabled.expect("enable");
    let row = listed
        .expect("list")
        .into_iter()
        .find(|d| d.id == id)
        .expect("the row is listed");
    assert_eq!(row.admitted, Some(false), "{row:?}");
    assert!(
        row.description.ends_with("pair it again"),
        "{}",
        row.description
    );
    assert_eq!(copy, Some(REFUSED), "its key admits from nowhere");
    assert!(forgotten.expect("forget"));
    stopped.expect("disable");
}
