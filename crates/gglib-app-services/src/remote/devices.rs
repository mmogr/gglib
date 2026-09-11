//! The three things a person asks about devices: invite one, forget one, see
//! them.
//!
//! Its own file rather than more of `serve.rs`, and along a real seam:
//! `serve.rs` is about *arming a tunnel*, this is about *who may use it*. The
//! two meet once — `invite` offers a code against a session `arm` started.
//!
//! What each of these does to the two stores is `enrolment.rs`'s business,
//! not this file's. Here is where the live tunnel is consulted: every one of
//! them peeks the serve slot, releases it, and only then writes — because a
//! settings write is slow and none of this may stall `status` for it.
//!
//! **Two stores, on purpose**, and neither lives here either. The readable
//! half — id, label, when it joined, when it was last seen — is in settings,
//! which is what the device list renders and what a person changes their mind
//! about; `roster.rs` reads and writes it. The keys are in a `0600` file
//! beside the endpoint identity, because `gglib config settings show` prints
//! settings unmasked by design and that output gets pasted into bug reports;
//! `device_keys.rs` reads and writes that, and seeds a listener from it.

use std::sync::Arc;

use tracing::{info, warn};

use super::RemoteOps;
use super::enrolment::{forget, offer};
use super::roster::read_roster;
use super::types::{DeviceView, Enabled};
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
    ///
    /// Answers with the whole [`Enabled`], not the
    /// [`OfferedPairing`](super::types::OfferedPairing) inside it, because
    /// the ticket is half of what a person is shown: the pairing screen
    /// draws it and the plain-text form prints it, and `OfferedPairing`
    /// does not carry one. The session already knows it, so
    /// handing it back costs nothing and saves every surface from splitting
    /// the pairing string to recover it.
    pub async fn invite(&self) -> Result<Enabled, GuiError> {
        let Some(enabled) = self.invite_if_up().await? else {
            return Err(GuiError::Conflict(
                "remote access is not enabled — run `gglib remote enable` first, or \
                 `gglib remote invite` to do both"
                    .to_owned(),
            ));
        };
        if enabled.pairing.is_none() {
            return Err(GuiError::Internal(
                "the invite came back without the code it armed".to_owned(),
            ));
        }
        Ok(enabled)
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
            // The live session's answer, not the caller's flag: this path
            // deliberately leaves the flags where `enable` set them.
            mcp_allowed: gglib_core::ports::RemoteGatewayPort::mcp_allowed(&*self.gateway),
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
        let gone = forget(self, handle.as_deref(), device).await?;

        // Peeked again, because the slot may have filled while the writes
        // above were waiting on `roster`. An `arm` that had already taken
        // both locks re-reads the key file and seeds from it — and if it read
        // before this call's write landed, it just put back the very key this
        // retired, on a listener the first peek could not see because the
        // slot was still a reservation. Telling the edge a second time is a
        // no-op in every other order, and this is the only one that needs it.
        let armed_since = {
            let live = self.live.lock().await;
            live.full().map(|l| Arc::clone(&l.handle))
        };
        if let Some(handle) = armed_since
            && handle.remove_token(device)
        {
            warn!(device = %device, "a tunnel armed mid-forget had seeded the retired key; removed");
        }

        // A code still on screen for this device would otherwise redeem for a
        // key the edge has just stopped holding: a device that pairs, sees a
        // green checkmark, and is refused on its first real request — the
        // least debuggable failure this has, and the one `offer`'s ordering
        // exists to prevent from the other end.
        if let Some(pending) = self.gateway.withdraw_pairing_for(device) {
            info!(device = %pending, "withdrew the open invite for a device being forgotten");
        }
        Ok(gone)
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
                admitted: admitting.as_ref().map(|names| names.contains(&d.id)),
                id: d.id,
                label: d.label,
                joined_at: d.joined_at,
                redeemed_at: d.redeemed_at,
                last_seen: d.last_seen,
            })
            .collect())
    }
}
