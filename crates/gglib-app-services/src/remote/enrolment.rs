//! Enrolling a device and retiring one: the two-store transaction underneath
//! `invite` and `forget`.
//!
//! Split from `devices.rs`, which is at its size budget, and by subject: that
//! file is about the three verbs and about consulting the live tunnel, and
//! this one is about the order two stores are written in and what unwinds
//! when a step fails. The ordering is the whole content — get it wrong and a
//! device pairs successfully and is refused on its first real request, which
//! is the least debuggable failure this feature has.
//!
//! Every write here takes `RemoteOps::roster` first and touches the key file
//! before the roster, so a concurrent invite and forget cannot interleave.

use std::sync::Arc;

use gglib_core::Device;
use tracing::{info, warn};

use super::RemoteOps;
use super::device_keys::{read_keys, write_keys};
use super::gateway::Offered;
use super::invite_watch;
use super::pairing::{MAX_ATTEMPTS_AT_EDGE, PAIRING_TTL};
use super::roster::{now_ms, read_roster, write_roster};
use super::types::OfferedPairing;
use crate::error::GuiError;

/// Invite a device that has not paired yet: a key of its own, held at the
/// tunnel edge, and a code that hands it over once.
///
/// The order is not free choice:
///
/// 1. `invite` **first**, because it is the call that can fail. modelpipe
///    mints the key and the device's name and holds the key at the edge, and
///    the code it mints is not redeemable yet.
/// 2. Both stores written next, before the code can be spent, so a daemon
///    that dies mid-redemption cannot leave a device holding a key this side
///    has forgotten. That is modelpipe's own rule for an invite: store the
///    key, then arm.
/// 3. The gateway holds the invite, which it refuses if one is already open
///    or the session moved under us.
/// 4. The code armed last. The edge answers it, counts wrong codes per
///    endpoint and expires it; `invite_watch` records the device when it
///    pairs.
///
/// Every failure after step 1 unwinds what came before it: `forget`'s
/// `remove_token` withdraws the invite as it drops the key.
///
/// # Errors
///
/// `Conflict` when an invite is already open or the session ended while this
/// was preparing; `Internal` when the edge refuses the invite or a store
/// cannot be written.
pub(super) async fn offer(
    ops: &RemoteOps,
    handle: &modelpipe::ServeHandle,
    epoch: u64,
) -> Result<OfferedPairing, GuiError> {
    let mut options = modelpipe::InviteOptions::default();
    options.ttl = PAIRING_TTL;
    options.wrong_codes = MAX_ATTEMPTS_AT_EDGE;
    let invited = handle
        .invite(options)
        .map_err(|e| GuiError::Internal(format!("the tunnel refused to invite a device: {e}")))?;
    let device = invited.device().to_owned();

    if let Err(e) = remember(ops, &device, invited.api_key()).await {
        // Both stores, not just the token: `remember` takes its key back out
        // of the file when the roster write fails, but not when that second
        // file write fails as well, and a key left there is listed as one
        // with no record until something retires it.
        forget_quietly(ops, handle, &device).await;
        return Err(e);
    }

    match ops
        .gateway
        .offer_pairing(epoch, device.clone(), Box::new(invited.handle()))
    {
        Offered::Armed => {}
        Offered::Superseded => {
            forget_quietly(ops, handle, &device).await;
            return Err(GuiError::Conflict(
                "remote access was taken down while the invite was being prepared".to_owned(),
            ));
        }
        Offered::AlreadyOpen => {
            forget_quietly(ops, handle, &device).await;
            return Err(GuiError::Conflict(
                "an invite is already open — wait for it to be used or to expire".to_owned(),
            ));
        }
    }

    // Outside the session lock, and safe there: a teardown landing now takes
    // the invite out of the gateway and withdraws it, and arming an invite
    // that has ended does nothing.
    invited.arm();
    invite_watch::watch(Arc::clone(&ops.gateway), device.clone(), invited.handle());

    info!(device = %device, "offered a pairing code for a new device");
    Ok(OfferedPairing {
        // From the invite rather than the caller's copy of the ticket: the
        // address set behind a ticket fills in over time, and this is the one
        // the code was minted against.
        pairing: invited.pairing().to_string(),
        code: invited.code().as_str().to_owned(),
        expires_in_s: PAIRING_TTL.as_secs(),
        device,
    })
}

/// Write both stores: the key to its file, the row to settings.
///
/// A roster write that fails takes the key back out of the file before the
/// guard is let go. Left to the caller's unwind, which has to wait for the
/// guard again, a `list` or `status` already queued behind this would read
/// the key with no row.
async fn remember(ops: &RemoteOps, id: &str, key: &str) -> Result<(), GuiError> {
    let _guard = ops.roster.lock().await;
    let mut keys = read_keys(ops)?;
    keys.insert(id.to_owned(), key.to_owned());
    write_keys(ops, &keys)?;

    let recorded = record(ops, id).await;
    if recorded.is_err() {
        keys.remove(id);
        if let Err(e) = write_keys(ops, &keys) {
            warn!(device = %id, "could not take back a key whose roster row was not written: {e}");
        }
    }
    recorded
}

/// Add an unspent invite's row to the roster.
async fn record(ops: &RemoteOps, id: &str) -> Result<(), GuiError> {
    let mut roster = read_roster(&ops.core).await?;
    roster.push(Device {
        id: id.to_owned(),
        label: None,
        joined_at: now_ms(),
        // Minted, not taken. `roster_sync` stamps this when the code is
        // redeemed, which is what tells an unspent invite from a device.
        redeemed_at: None,
        last_seen: None,
        peer: None,
    });
    write_roster(&ops.core, roster).await
}

/// Stop admitting `id`, and forget it.
///
/// The edge first, then the key file, then the roster: `remember`'s order
/// reversed. The roster goes last because it is only what `list` shows, while
/// the other two are what admit — so stopping part-way leaves a row that
/// nothing admits, and never a key admitted that no row accounts for, which no
/// surface would show and nobody would know to retire.
///
/// While the tunnel is up, that row reads "not admitted". Stopped before the
/// key file is written, the file still holds the key, and the next arm seeds
/// it back as though this never ran; stopped before the roster is written, the
/// key is gone for good and the row stays. With the tunnel down no
/// `remove_token` ran, so a stop before the key file has changed nothing, and
/// `list` says "tunnel down" of every row. Either way, running `forget` again
/// finishes the job.
///
/// # Errors
///
/// `Internal` when a store cannot be written.
pub(super) async fn forget(
    ops: &RemoteOps,
    handle: Option<&modelpipe::ServeHandle>,
    id: &str,
) -> Result<bool, GuiError> {
    if let Some(handle) = handle {
        handle.remove_token(id);
    }
    let _guard = ops.roster.lock().await;
    let mut keys = read_keys(ops)?;
    let had_key = keys.remove(id).is_some();
    // Each store is written only if this changed it. Retiring a device this
    // machine never issued a key to is an answer, not an edit: replacing two
    // files to record nothing is work whose only effects are the ones that
    // can go wrong. The key store is process-wide, so a no-op forget would
    // still take its turn at replacing it.
    if had_key {
        write_keys(ops, &keys)?;
    }

    let mut roster = read_roster(&ops.core).await?;
    let before = roster.len();
    roster.retain(|d| d.id != id);
    let had_row = roster.len() != before;
    if had_row {
        write_roster(&ops.core, roster).await?;
    }
    Ok(had_key || had_row)
}

/// Unwind a half-made invite. Failures are logged, not raised: the caller is
/// already returning an error and the one that matters is theirs.
async fn forget_quietly(ops: &RemoteOps, handle: &modelpipe::ServeHandle, id: &str) {
    if let Err(e) = forget(ops, Some(handle), id).await {
        warn!(device = %id, "could not unwind a device that was never paired: {e}");
    }
}

#[cfg(test)]
#[path = "enrolment_tests.rs"]
mod enrolment_tests;
