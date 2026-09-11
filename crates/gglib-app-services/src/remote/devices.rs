//! Who may come through the tunnel: inviting a device, forgetting one, and
//! listing what this machine has issued a key to.
//!
//! Its own file rather than more of `serve.rs`, which is at its size budget,
//! and along a real seam: `serve.rs` is about *arming a tunnel*, and this is
//! about *who may use it*. The two meet once — `invite` offers a code
//! against a session `arm` started.
//!
//! **Two stores, on purpose**, and neither lives here. The readable half —
//! id, label, when it joined, when it was last seen — is in settings, which
//! is what the device list renders and what a person changes their mind
//! about; `roster.rs` reads and writes it. The keys are in a `0600` file
//! beside the endpoint identity, because `gglib config settings show` prints
//! settings unmasked by design and that output gets pasted into bug reports;
//! `device_keys.rs` reads and writes that, and seeds a listener from it.
//!
//! Every write here takes `RemoteOps::roster` first and touches the key file
//! before the roster, so the two cannot be interleaved by a concurrent
//! invite and forget.

use std::sync::Arc;

use gglib_core::Device;
use gglib_core::access::{generate_api_key, generate_device_id};
use tracing::{info, warn};

use super::RemoteOps;
use super::device_keys::{read_keys, write_keys};
use super::gateway::Offered;
use super::pairing::{MAX_ATTEMPTS_AT_EDGE, PAIRING_TTL};
use super::roster::{now_ms, read_roster, write_roster};
use super::types::{DeviceView, Enabled, OfferedPairing};
use crate::error::GuiError;

impl RemoteOps {
    /// Offer a code that hands one new device a key of its own.
    ///
    /// The serve slot is read and released before the offer rather than held
    /// across it: [`offer`] writes two stores, and a settings write is slow
    /// enough that holding the slot would stall `status` for it. The epoch
    /// read under the lock is what makes that safe — a teardown landing in
    /// between supersedes the offer, which is refused, rather than leaving a
    /// live code on a session that has ended.
    ///
    /// # Errors
    ///
    /// `Conflict` when the tunnel is not up, when an invite is already open,
    /// or when remote access went down while this was preparing; `Internal`
    /// when a store cannot be written or the edge refuses the token or the
    /// grant.
    pub async fn invite(&self) -> Result<OfferedPairing, GuiError> {
        let Some(enabled) = self.invite_if_up().await? else {
            return Err(GuiError::Conflict(
                "remote access is not enabled — `gglib remote enable --invite` does both"
                    .to_owned(),
            ));
        };
        enabled.pairing.ok_or_else(|| {
            GuiError::Internal("the invite came back without the code it armed".to_owned())
        })
    }

    /// The same, against a tunnel that may or may not be up: `Ok(None)` means
    /// there was nothing to invite onto.
    ///
    /// Shared with `enable --invite`, which is what makes that command work
    /// on a machine that is already serving instead of answering "already
    /// enabled". Without it a person who needs to pair a second device is
    /// told to `disable` first, which drops every device already using the
    /// tunnel — and every string in this codebase that points at
    /// `enable --invite` would be pointing at a refusal.
    ///
    /// The flags in that request are *not* applied to a session already
    /// running; only the code is new. `disable` and `enable` again to change
    /// them, which is the one thing that has to be said out loud, because
    /// `--allow-mcp` alongside `--invite` would otherwise look like it took.
    pub(super) async fn invite_if_up(&self) -> Result<Option<Enabled>, GuiError> {
        let armed = {
            let live = self.live.lock().await;
            live.full().map(|l| (Arc::clone(&l.handle), l.epoch))
        };
        let Some((handle, epoch)) = armed else {
            return Ok(None);
        };
        let ticket = handle.ticket().to_string();
        let pairing = offer(self, &handle, epoch, &ticket).await?;
        Ok(Some(Enabled {
            ticket,
            pairing: Some(pairing),
        }))
    }

    /// Stop admitting one device and forget it. `false` when this machine
    /// held nothing under that name.
    ///
    /// Works with the tunnel down, and must: a laptop is lost at a moment
    /// nobody chose, and a `forget` that needed the tunnel up would be one
    /// more thing to do first. With no listener to tell, both stores are
    /// still written, and the next arm seeds from them.
    ///
    /// # Errors
    ///
    /// `Internal` when a store cannot be written.
    pub async fn forget(&self, device: &str) -> Result<bool, GuiError> {
        let handle = {
            let live = self.live.lock().await;
            live.full().map(|l| Arc::clone(&l.handle))
        };
        self::forget(self, handle.as_deref(), device).await
    }

    /// Every device this machine has issued a key to, and whether the
    /// listener is actually holding each one.
    ///
    /// The two can disagree — [`device_keys::seed`](super::device_keys::seed)
    /// skips a row the edge refuses rather
    /// than failing the arm — and a row that admits nothing is exactly what
    /// a person needs to see, because everything else about it looks fine.
    ///
    /// # Errors
    ///
    /// `Internal` when the roster cannot be read.
    pub async fn list(&self) -> Result<Vec<DeviceView>, GuiError> {
        let admitting = {
            let live = self.live.lock().await;
            live.full().map(|l| l.handle.token_names())
        };
        let roster = read_roster(&self.core).await?;
        Ok(roster
            .into_iter()
            .map(|d| DeviceView {
                // `None` with the tunnel down: nothing admits then, and
                // saying `false` would read as "this device was dropped".
                admitted: admitting
                    .as_ref()
                    .map(|names| names.iter().any(|name| *name == d.id)),
                id: d.id,
                label: d.label,
                joined_at: d.joined_at,
                last_seen: d.last_seen,
            })
            .collect())
    }
}

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
    // Two authorities can disagree — a `resume` seeds from a file a
    // concurrent invite has since changed — and the listener is the one that
    // actually admits, so its refusal is a redraw rather than a failure.
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
