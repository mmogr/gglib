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

use gglib_core::Device;
use gglib_core::access::{generate_api_key, generate_device_id};
use tracing::{info, warn};

use super::RemoteOps;
use super::device_keys::{read_keys, write_keys};
use super::gateway::Offered;
use super::pairing::{MAX_ATTEMPTS_AT_EDGE, PAIRING_TTL};
use super::roster::{now_ms, read_roster, write_roster};
use super::types::OfferedPairing;
use crate::error::GuiError;

/// Mint a key for a device that has not paired yet, and a code that hands it
/// over once.
///
/// The order is not free choice:
///
/// 1. `add_token` **first**, because it is the call that can fail. A code
///    redeemable before its token exists means a device pairs successfully
///    and is then refused on its first real request — a 401 immediately
///    after a green checkmark, the least debuggable failure this has.
/// 2. Both stores written next, before the code can be spent, so a daemon
///    that dies mid-redemption cannot leave a device holding a key this side
///    has forgotten.
/// 3. The gateway armed, which refuses if a pairing is already open or if
///    the session moved under us.
/// 4. The edge grant last, bounded: the ticket lasts now, so anyone holding
///    it can wait for a window and spend it guessing — as wrong *bearers*,
///    which the local counter never sees and the edge does.
///
/// Every failure after step 1 unwinds what came before it.
///
/// # Errors
///
/// `Conflict` when a pairing is already open or the session ended while this
/// was preparing; `Internal` when a store cannot be written or the edge
/// refuses the token or the grant.
pub(super) async fn offer(
    ops: &RemoteOps,
    handle: &modelpipe::ServeHandle,
    epoch: u64,
    ticket: &str,
) -> Result<OfferedPairing, GuiError> {
    let (id, key) = mint(handle)?;

    if let Err(e) = remember(ops, &id, &key).await {
        // Both stores, not just the token: `remember` writes the key file
        // first and the roster second, so a roster failure leaves a key
        // behind that `seed` would install at the next arm — an id admitted
        // at the edge that no roster row accounts for and no `list` shows.
        forget_quietly(ops, handle, &id).await;
        return Err(e);
    }

    let code = gglib_core::access::generate_pairing_code();
    match ops
        .gateway
        .offer_pairing(epoch, code.clone(), key, id.clone(), PAIRING_TTL)
    {
        Offered::Armed => {}
        Offered::Superseded => {
            forget_quietly(ops, handle, &id).await;
            return Err(GuiError::Conflict(
                "remote access was taken down while the invite was being prepared".to_owned(),
            ));
        }
        Offered::AlreadyOpen => {
            forget_quietly(ops, handle, &id).await;
            return Err(GuiError::Conflict(
                "an invite is already open — wait for it to be used or to expire".to_owned(),
            ));
        }
    }

    if let Err(e) = handle.grant_once_bounded(code.clone(), PAIRING_TTL, MAX_ATTEMPTS_AT_EDGE) {
        ops.gateway.withdraw_pairing(epoch);
        forget_quietly(ops, handle, &id).await;
        return Err(GuiError::Internal(format!(
            "could not arm the pairing code: {e}"
        )));
    }

    info!(device = %id, "offered a pairing code for a new device");
    Ok(OfferedPairing {
        pairing: format!("{ticket}-{code}"),
        code,
        expires_in_s: PAIRING_TTL.as_secs(),
        device: id,
    })
}

/// Mint an id nothing already holds, and a key, and hold them at the edge.
fn mint(handle: &modelpipe::ServeHandle) -> Result<(String, String), GuiError> {
    let key = generate_api_key();
    // The listener is the authority on what admits, so a refusal here is a
    // redraw rather than a failure: an id it will not hold is one no device
    // could have used anyway.
    for _ in 0..5 {
        let id = generate_device_id();
        match handle.add_token(&id, key.clone()) {
            Ok(()) => return Ok((id, key)),
            Err(e) => warn!("a minted device id was refused, drawing another: {e}"),
        }
    }
    Err(GuiError::Internal(
        "could not mint a device id the tunnel would hold".to_owned(),
    ))
}

/// Write both stores: the key to its file, the row to settings.
async fn remember(ops: &RemoteOps, id: &str, key: &str) -> Result<(), GuiError> {
    let _guard = ops.roster.lock().await;
    let mut keys = read_keys()?;
    keys.insert(id.to_owned(), key.to_owned());
    write_keys(&keys)?;

    let mut roster = read_roster(&ops.core).await?;
    roster.push(Device {
        id: id.to_owned(),
        label: None,
        joined_at: now_ms(),
        last_seen: None,
    });
    write_roster(&ops.core, roster).await
}

/// Stop admitting `id`, and forget it.
///
/// The edge first and settings second: `remove_token` is what actually stops
/// admission, and a crash between the two leaves a device gone from the
/// roster but still admitted until the next arm — the safer way round than
/// the reverse.
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
    let mut keys = read_keys()?;
    let had_key = keys.remove(id).is_some();
    write_keys(&keys)?;

    let mut roster = read_roster(&ops.core).await?;
    let before = roster.len();
    roster.retain(|d| d.id != id);
    let had_row = roster.len() != before;
    write_roster(&ops.core, roster).await?;
    Ok(had_key || had_row)
}

/// Unwind a half-made invite. Failures are logged, not raised: the caller is
/// already returning an error and the one that matters is theirs.
async fn forget_quietly(ops: &RemoteOps, handle: &modelpipe::ServeHandle, id: &str) {
    if let Err(e) = forget(ops, Some(handle), id).await {
        warn!(device = %id, "could not unwind a device that was never paired: {e}");
    }
}
