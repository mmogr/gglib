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
use super::pairing::Offer;
use super::resume_wait::Waited;
use super::roster::read_roster;
use super::slot::Busy;
use super::types::{DeviceView, Enabled};
use crate::error::GuiError;
use gglib_core::Device;

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
    /// `Conflict` when the tunnel is not up, or is still coming up once the
    /// wait for the daemon's own resume is over, when an invite is already
    /// open, or when remote access went down while this was preparing; `Internal` when a store cannot be written or the edge
    /// refuses the token or the grant.
    ///
    /// Answers with the whole [`Enabled`], not the
    /// [`OfferedPairing`](super::types::OfferedPairing) inside it, because
    /// the ticket is half of what a person is shown: the pairing screen
    /// draws it and the plain-text form prints it, and `OfferedPairing`
    /// does not carry one. The session already knows it, so
    /// handing it back costs nothing and saves every surface from splitting
    /// the pairing string to recover it.
    pub async fn invite(&self) -> Result<Enabled, GuiError> {
        // A daemon this command started for itself is putting the tunnel
        // back, and an invite typed a few seconds later should get the code
        // `enable --invite` gets rather than a refusal. A `disable` while it
        // waits ends it at once: the slot may still read as arming until
        // that `disable` reaches it, and a code on a session about to go
        // would leave a row nobody can redeem.
        if self.wait_out_the_resume().await == Waited::Cancelled {
            return Err(GuiError::Conflict(
                "the invite was cancelled by `gglib remote disable`, which switched remote access \
                 off while this waited for it"
                    .to_owned(),
            ));
        }
        let Some(enabled) = self.answer_if_up(Offer::Code).await? else {
            return Err(self.nothing_to_invite_onto().await);
        };
        if enabled.pairing.is_none() {
            return Err(GuiError::Internal(
                "the invite came back without the code it armed".to_owned(),
            ));
        }
        Ok(enabled)
    }

    /// Why [`Self::answer_if_up`] found nothing, read afresh rather than
    /// returned by it: `enable --invite` shares that call, and has to go on
    /// getting `Ok(None)` for every way the tunnel can be down, because that
    /// is what lets it arm one.
    ///
    /// Settings first and the serve slot second, the order `status` reads
    /// them in. A settings read that fails counts as the switch being off,
    /// which leaves the refusal saying what it always said.
    async fn nothing_to_invite_onto(&self) -> GuiError {
        let switched_on = self
            .core
            .settings()
            .get()
            .await
            .is_ok_and(|s| s.remote_enabled == Some(true));
        let busy = self.live.lock().await.busy();
        refusal_without_a_tunnel(busy.as_ref(), switched_on)
    }

    /// The session as it stands, against a tunnel that may or may not be up:
    /// `Ok(None)` means there was nothing to answer from.
    ///
    /// Shared with `enable --invite`, which is what makes that command work
    /// on a machine that is already serving instead of answering "already
    /// enabled". Without it a person who needs to pair a second device is
    /// told to `disable` first, which drops every device already using the
    /// tunnel — and every string in this codebase that points at
    /// `enable --invite` would be pointing at a refusal. `enable` also
    /// answers from here after waiting out the daemon's own resume, and a
    /// plain `enable` then asks for `Offer::Silent`: the session, no code.
    ///
    /// The flags in that request are *not* applied to a session already
    /// running; only the code is new. `disable` and `enable` again to change
    /// them, which is the one thing that has to be said out loud, because
    /// `--allow-mcp` alongside `--invite` would otherwise look like it took.
    pub(super) async fn answer_if_up(&self, wanted: Offer) -> Result<Option<Enabled>, GuiError> {
        let armed = {
            let live = self.live.lock().await;
            live.full().map(|l| (Arc::clone(&l.handle), l.epoch))
        };
        let Some((handle, epoch)) = armed else {
            return Ok(None);
        };
        let ticket = handle.ticket().to_string();
        let pairing = match wanted {
            Offer::Code => Some(offer(self, &handle, epoch, &ticket).await?),
            Offer::Silent => None,
        };
        Ok(Some(Enabled {
            ticket,
            pairing,
            // The live session's answer, not the caller's flag: this path
            // deliberately leaves the flags where `enable` set them.
            mcp_allowed: gglib_core::ports::RemoteGatewayPort::mcp_allowed(&*self.gateway),
            // This *is* the path that finds it already up; there is no other.
            already_up: true,
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
        // First, before the edge is told anything or either store is touched.
        // A code still on screen for this device would otherwise stay
        // redeemable across every wait below — the roster lock and two store
        // writes — for a key the edge had already dropped: a device that
        // pairs, sees a green checkmark, and is refused on its first real
        // request, the least debuggable failure this has and the one
        // `offer`'s ordering exists to prevent from the other end. Withdrawn
        // last, as it was, it was also skipped whenever a store write failed,
        // which left the code live for the rest of its two minutes.
        if let Some(pending) = self.gateway.withdraw_pairing_for(device) {
            info!(device = %pending, "withdrew the open invite for a device being forgotten");
        }

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
        // Settings first and the serve slot second, the order `status` reads
        // them in. Taken the other way round, a row read mid-`forget` could
        // come back "admitted" for a key the edge had already dropped.
        let roster = read_roster(&self.core).await?;
        let admitting = {
            let live = self.live.lock().await;
            live.full().map(|l| l.handle.token_names())
        };
        Ok(viewed(roster, admitting.as_deref()))
    }
}

/// The refusal for an invite that found no tunnel up, as the person who asked
/// needs to hear it.
///
/// Three sentences, because the fix differs:
/// - With the switch off, `enable` is the fix.
/// - With a tunnel still arming — another command's `enable`, or the
///   daemon's own resume outlasting the wait `invite` gives it — the fix is
///   to wait. A second `enable` refuses while one is arming, so naming it
///   would send someone to a refusal; `disable` is the way out, as
///   `busy_serving` says it.
/// - With the switch on and nothing reserved, the resume that puts the
///   tunnel back has given up, or had not begun when this looked, since
///   `invite` has already waited out one that was working. `enable --invite`
///   is the fix either way: it arms the tunnel again with a code, and waits
///   for a resume that has only just begun.
///
/// `Full` means an arm finished between the two reads: it was still coming up
/// when `answer_if_up` looked, and an invite works now, so the same advice
/// holds.
fn refusal_without_a_tunnel(busy: Option<&Busy>, switched_on: bool) -> GuiError {
    GuiError::Conflict(
        match (busy, switched_on) {
            (Some(Busy::Filling | Busy::Full), _) => {
                "remote access is still coming up — `gglib remote status` shows when the \
                 ticket is ready; run `gglib remote invite` then, or `gglib remote disable` \
                 to give up on it"
            }
            (None, true) => {
                "remote access is switched on, but nothing is bound and nothing is arming — \
                 `gglib remote enable --invite` arms it again, with the flags you give it, and \
                 offers a code"
            }
            (None, false) => {
                "remote access is not enabled — run `gglib remote enable` first, or \
                 `gglib remote enable --invite` to do both"
            }
        }
        .to_owned(),
    )
}

/// Roster rows as the surfaces see them, given what the edge is admitting.
///
/// Shared with [`RemoteOps::status`](super::RemoteOps::status), which
/// answers the same rows off the settings record it has already read. Two
/// spellings of "is this device admitted" would be two chances to answer it
/// differently, on the one question a person is asking the list.
///
/// Both callers read settings before the serve slot, so `admitted` is what
/// the edge held when the slot was read — the later of the two reads.
/// Mid-`forget` a row can come back "not admitted" for one read before it is
/// gone; a device the edge had already dropped by then is never called
/// admitted.
pub(super) fn viewed(roster: Vec<Device>, admitting: Option<&[String]>) -> Vec<DeviceView> {
    roster
        .into_iter()
        .map(|d| DeviceView {
            // `None` with the tunnel down: nothing admits then, and saying
            // `false` would read as "this device was dropped".
            admitted: admitting.map(|names| names.contains(&d.id)),
            id: d.id,
            label: d.label,
            joined_at: d.joined_at,
            redeemed_at: d.redeemed_at,
            last_seen: d.last_seen,
        })
        .collect()
}

#[cfg(test)]
#[path = "devices_tests.rs"]
mod devices_tests;
