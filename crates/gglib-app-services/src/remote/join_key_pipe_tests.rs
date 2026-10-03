//! A joining machine is one endpoint to the machine it joins, across a
//! disconnect and a restart, over a real pipe.
//!
//! The fixture `invite_watch_tests.rs` uses: a serving side from
//! `serve_watch_tests.rs` that binds its endpoint without discovery, and a
//! joiner that dials it with discovery off. Neither names a relay, so each
//! endpoint uses n0's public ones when it can reach them, as that file's do.
//! Here the joiner is a `RemoteOps` of its own, so every dial is `join`'s.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use gglib_core::SettingsUpdate;
use gglib_core::services::AppCore;
use modelpipe::Ticket;

use super::super::RemoteOps;
use super::super::serve_watch_tests::{offline, ops_with_key};
use super::super::types::{EnableRequest, JoinRequest};
use crate::test_support_remote::{
    scratch_join_keys, test_remote_ops_joining_from, within_a_moment,
};

/// A joining `RemoteOps` and the settings it keeps its pairing in.
type Joiner = (Arc<AppCore>, Arc<RemoteOps>);

/// What the serving side saw of each join, and the key file after the first.
struct Seen {
    paired: Option<String>,
    key: fs::Metadata,
    key_dir: fs::Metadata,
    /// Whether the serving side had no peer left after each disconnect, so
    /// that the one peer it held after the next join was that join's.
    emptied: [bool; 4],
    again: Option<String>,
    restarted: Option<String>,
    fresh: Option<String>,
}

/// The fingerprint of the one peer `serving` holds, once it holds exactly
/// one.
async fn the_one_peer(serving: &RemoteOps) -> Option<String> {
    within_a_moment(async || match serving.status().await.peers.as_slice() {
        [one] => Some(one.fingerprint.clone()),
        _ => None,
    })
    .await
}

/// Whether `serving` holds no peer.
async fn none_left(serving: &RemoteOps) -> bool {
    within_a_moment(async || serving.status().await.peers.is_empty().then_some(()))
        .await
        .is_some()
}

/// A join on a free loopback port with discovery off: from `pairing`, or
/// from the stored pairing when it is `None`.
fn request(pairing: Option<String>) -> JoinRequest {
    JoinRequest {
        pairing,
        port: Some(0),
        discovery: false,
        ..JoinRequest::default()
    }
}

/// `joiner` joins with no code and disconnects: the one peer `serving` held
/// while it was joined, and whether it held none once it had gone.
async fn join_and_leave(
    serving: &RemoteOps,
    joiner: &RemoteOps,
    which: &str,
) -> Result<(Option<String>, bool), String> {
    joiner
        .join(request(None))
        .await
        .map_err(|e| format!("{which}: {e}"))?;
    let seen = the_one_peer(serving).await;
    joiner
        .disconnect()
        .await
        .map_err(|e| format!("{which}, disconnecting: {e}"))?;
    Ok((seen, none_left(serving).await))
}

/// Every join the test makes, one at a time, and what `serving` saw of each.
///
/// The first joiner pairs from `pairing`, disconnects, and joins again with
/// no code. The second keeps its keys where the first did, which is a daemon
/// restarted over the same data root; the third keeps its own. Both are
/// given the pairing the first stored.
async fn gather(
    serving: &RemoteOps,
    pairing: String,
    key: &Path,
    [
        (first_core, first),
        (restarted_core, restarted),
        (fresh_core, fresh),
    ]: &[Joiner; 3],
) -> Result<Seen, String> {
    first
        .join(request(Some(pairing)))
        .await
        .map_err(|e| format!("pairing: {e}"))?;
    let paired = the_one_peer(serving).await;
    let key_file = fs::metadata(key).map_err(|e| format!("the key file: {e}"))?;
    let key_dir = key.parent().ok_or("the key has no directory")?;
    let key_dir = fs::metadata(key_dir).map_err(|e| format!("its directory: {e}"))?;
    first
        .disconnect()
        .await
        .map_err(|e| format!("disconnecting: {e}"))?;
    let unpaired = none_left(serving).await;
    let (again, left_again) = join_and_leave(serving, first, "joining again").await?;

    let stored = first_core
        .settings()
        .get()
        .await
        .map_err(|e| format!("reading the pairing: {e}"))?
        .remote_pairing
        .ok_or("pairing stored no pairing")?;
    for core in [restarted_core, fresh_core] {
        core.settings()
            .update(SettingsUpdate {
                remote_pairing: Some(Some(stored.clone())),
                ..SettingsUpdate::default()
            })
            .await
            .map_err(|e| format!("copying the pairing: {e}"))?;
    }
    let (after_restart, left_restarted) =
        join_and_leave(serving, restarted, "joining after a restart").await?;
    let (from_fresh, left_fresh) =
        join_and_leave(serving, fresh, "joining with keys of its own").await?;
    Ok(Seen {
        paired,
        key: key_file,
        key_dir,
        emptied: [unpaired, left_again, left_restarted, left_fresh],
        again,
        restarted: after_restart,
        fresh: from_fresh,
    })
}

/// The serving side sees one endpoint for a machine that joins it: after a
/// disconnect and a join with no code, and after a restart that keeps the
/// same keys. A joiner with keys of its own is another endpoint, which is
/// what shows that the comparisons can fail.
///
/// Each check waits for the serving side to hold exactly one peer, after
/// waiting for the previous join's peer to leave, so neither an empty list
/// nor a connection left over from the join before can pass for the one
/// asked about. **From the `enable` on, everything fallible is judged after
/// the cleanup**, as in `invite_watch_tests.rs`: this mints a device key into
/// the key file, and a panic before the `forget` would leave it there.
#[tokio::test(flavor = "multi_thread")]
async fn a_joining_machine_is_one_endpoint_across_a_disconnect_and_a_restart() {
    let (_core, _proxy, _events, serving, _arming) = ops_with_key().await;
    let dir = scratch_join_keys();
    let (first_core, first, _) = test_remote_ops_joining_from(dir.clone()).await;
    let (restarted_core, restarted, _) = test_remote_ops_joining_from(dir.clone()).await;
    let (fresh_core, fresh, _) = test_remote_ops_joining_from(scratch_join_keys()).await;
    let joiners = [
        (first_core, Arc::clone(&first)),
        (restarted_core, Arc::clone(&restarted)),
        (fresh_core, Arc::clone(&fresh)),
    ];
    let enabled = serving
        .enable(EnableRequest {
            invite: true,
            ..offline()
        })
        .await;

    // Gather.
    let offered = enabled.as_ref().ok().and_then(|e| e.pairing.clone());
    let seen = match (&enabled, &offered) {
        (Ok(enabled), Some(offered)) => match enabled.ticket.parse::<Ticket>() {
            Ok(far) => {
                let key = dir.join(far.fingerprint());
                gather(&serving, offered.pairing.clone(), &key, &joiners).await
            }
            Err(e) => Err(format!("reading the ticket it offered: {e}")),
        },
        _ => Err("enabling with an invite".to_owned()),
    };

    // Clean up.
    for joiner in [&first, &restarted, &fresh] {
        let _ = joiner.disconnect().await;
    }
    let forgotten = match &offered {
        Some(offered) => Some(serving.forget(&offered.device).await),
        None => None,
    };
    let stopped = serving.disable().await;

    // Judge.
    let seen = seen.expect("every join went through");
    let paired = seen
        .paired
        .expect("the serving side never held exactly one peer after the pairing");
    assert_eq!(seen.key.len(), 53, "not the key modelpipe writes");
    assert!(seen.key_dir.is_dir(), "the key's directory is not one");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(seen.key.permissions().mode() & 0o777, 0o600);
        assert_eq!(seen.key_dir.permissions().mode() & 0o777, 0o700);
    }
    assert_eq!(seen.emptied, [true; 4], "a peer outlived its disconnect");
    assert_eq!(
        seen.again.as_deref(),
        Some(paired.as_str()),
        "a join after a disconnect was another endpoint"
    );
    assert_eq!(
        seen.restarted.as_deref(),
        Some(paired.as_str()),
        "a join after a restart was another endpoint"
    );
    let fresh = seen
        .fresh
        .expect("the serving side never held exactly one peer");
    assert_ne!(
        fresh, paired,
        "a joiner with keys of its own was the same endpoint"
    );
    let forgotten = forgotten.expect("an invite was offered");
    assert!(
        forgotten.expect("forget"),
        "the device this minted was held"
    );
    stopped.expect("disable");
}

/// A join with a code over a stored pairing for another machine answers with
/// that machine's name, and with the name of the machine it joined, read
/// from that machine's own `/v1/models` once the connection was installed
/// and kept on the new pairing: through `join` and `dial` over the
/// in-process pipe, to the serving side's real proxy. That proxy runs here,
/// so the name it gives is this host's.
#[tokio::test(flavor = "multi_thread")]
async fn a_join_names_the_machine_it_joined_and_the_one_it_replaced() {
    use crate::test_support_remote::{KEY_B, TICKET_B, paired_with};
    let (_core, _proxy, _events, serving, _arming) = ops_with_key().await;
    let (joiner_core, joiner, _) = test_remote_ops_joining_from(scratch_join_keys()).await;
    let mut machine_b = paired_with(TICKET_B, KEY_B);
    if let Some(Some(pairing)) = machine_b.remote_pairing.as_mut() {
        pairing.name = Some("desk-b".to_owned());
    }
    joiner_core
        .settings()
        .update(machine_b)
        .await
        .expect("machine B's pairing is stored");
    let enabled = serving
        .enable(EnableRequest {
            invite: true,
            ..offline()
        })
        .await;
    let offered = enabled.as_ref().ok().and_then(|e| e.pairing.clone());
    let answer = match &offered {
        Some(offered) => Some(joiner.join(request(Some(offered.pairing.clone()))).await),
        None => None,
    };
    let _ = joiner.disconnect().await;
    if let Some(offered) = &offered {
        let _ = serving.forget(&offered.device).await;
    }
    let _ = serving.disable().await;
    let answer = answer
        .expect("an invite was offered")
        .expect("the join went through");
    assert!(answer.paired, "a code was redeemed");
    assert_eq!(
        answer.replaced.as_deref(),
        Some("desk-b"),
        "dial dropped the pairing it replaced"
    );
    // Unnamed, the proxy gives no name and the comparisons below would pass
    // whether or not the read happened.
    let named = gglib_proxy::models::this_machine_name()
        .expect("this host's name is not a plain label, so the proxy gives none to read");
    assert_eq!(
        answer.name.as_deref(),
        Some(named.as_str()),
        "the joined machine's name was not read"
    );
    let stored = joiner_core.settings().get().await.expect("settings load");
    assert_eq!(stored.remote_pairing.and_then(|p| p.name), Some(named));
}
