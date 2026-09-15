//! Starting, replacing and ending a gateway session, and the invite it holds.
//!
//! A child of `gateway.rs` rather than a sibling, for two reasons. The state
//! these methods move is private to [`RemoteGateway`] and should stay that
//! way — a sibling module would need every field opened to the whole of
//! `remote`. And they are one subject: every one of them is the same three
//! lines of reasoning about an epoch.
//!
//! **What the epoch is for.** `enable`, `invite` and the teardown all release
//! the serve slot before they finish — arming takes fifteen seconds and a
//! drain takes five, and holding the lock across either would stop
//! `gglib remote status` answering for that long. So they overlap: a
//! `disable` can be draining while a fresh `enable` arms, and an `invite` can
//! be minting a key for a session that ends before it comes back. Each of
//! these methods is therefore handed the epoch its caller saw, reads the
//! current one under the same lock it acts under, and does nothing when they
//! differ.
//!
//! What that buys is the absence of two specific failures, both of which
//! reach a person: an invite held open for a session with no listener behind
//! it, which `status` would report as a code nobody can redeem, and a fresh
//! session silently reset by the teardown of the one it replaced, which
//! withdraws the code the operator is holding.
//!
//! **Whoever takes an invite out of the slot records how it ended.** The
//! redemption happens at the tunnel edge, so this process learns that a
//! device paired only by reading the invite's outcome. `invite_watch` reads
//! it as soon as it is set, but that is a task, and a reset or a new offer
//! can take the invite out first. So every path that takes one out withdraws
//! it, which is a no-op once it has ended, and then records whatever it ended
//! as. A device that redeemed is recorded once, by whichever got there first.

use std::sync::atomic::Ordering;

use gglib_core::events::AppEvent;
use gglib_core::ports::RemoteGatewayPort;
use modelpipe::InviteOutcome;
use tracing::{info, warn};

use super::RemoteGateway;
use crate::remote::pairing::{Invitation, Open};
use crate::remote::roster::{Note, now_ms};

/// What [`RemoteGateway::offer_pairing`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::remote) enum Offered {
    /// Held: the invite is this session's, and the caller may arm its code.
    Armed,
    /// A later session owns the gateway; nothing was held.
    Superseded,
    /// An invite is already open on this session; nothing was held.
    AlreadyOpen,
}

impl RemoteGateway {
    /// Arm a session — `/mcp` is open to tunnelled requests or it is not —
    /// and say which session that is. The number comes back for
    /// [`Self::reset_session_if`].
    ///
    /// The paired flag is cleared here as well as there, because a teardown
    /// is not guaranteed to run: the session this replaces may still be
    /// draining, and its teardown will decline to touch anything (that is
    /// what the epoch is for). Nobody has paired with a session that is only
    /// now being armed, and `status` would otherwise report the last one's
    /// answer.
    ///
    /// No invite is opened here. `enable` starts a session; `invite` is what
    /// offers a code, against a session that is already up — so an invite
    /// the previous session left open is withdrawn rather than inherited, and
    /// a restart that puts the tunnel back offers nothing at all.
    pub(in crate::remote) fn begin_session(&self, allow_mcp: bool) -> u64 {
        let mut session = self.session();
        *session += 1;
        self.close_invite(self.pairing.take());
        self.mcp_allowed.store(allow_mcp, Ordering::Relaxed);
        self.paired.store(false, Ordering::Relaxed);
        *session
    }

    /// Hold `invitation`, minted for `device`, as the open invite on the
    /// session `epoch` names.
    ///
    /// Epoch-guarded, for [`Self::reset_session_if`]'s reason from the other
    /// side: `invite` reads the live slot, releases it, mints a key and
    /// writes it, then comes back. A `disable` in that window leaves `epoch`
    /// naming a session that is gone, and an invite held for it would read
    /// as a live code on a tunnel that is down. Read and act under the one
    /// lock, so an `enable` landing between the check and the hold cannot be
    /// held over.
    ///
    /// One invite at a time is this machine's rule, not modelpipe's, which
    /// would take several. An invite that has ended but not been recorded
    /// yet is recorded now rather than dropped by the one replacing it.
    ///
    /// **The paired flag is cleared here, not only when a session begins or
    /// ends.** It answers "has the code now on offer been taken?", and a
    /// session outlives any one code: `invite` offers a second one against a
    /// tunnel that is already up, which is the point of offering a code
    /// without taking every other device down. Left set from the first
    /// device, it would have the pairing screen report success — naming that
    /// first device, off `last_peer` — within a second of the second code
    /// being shown, and stop watching while the code stayed live and
    /// redeemable for the rest of its `ttl`.
    pub(in crate::remote) fn offer_pairing(
        &self,
        epoch: u64,
        device: String,
        invitation: Box<dyn Invitation>,
    ) -> Offered {
        let session = self.session();
        if *session != epoch {
            return Offered::Superseded;
        }
        if self.pairing.active() {
            return Offered::AlreadyOpen;
        }
        self.close_invite(self.pairing.take());
        self.pairing.begin(device, invitation);
        self.paired.store(false, Ordering::Relaxed);
        Offered::Armed
    }

    /// Record how the invite minted for `device` ended, unless another path
    /// took it out of the slot first. `invite_watch` calls this once the
    /// invite has ended.
    pub(in crate::remote) fn settle_invite(&self, device: &str) {
        let _session = self.session();
        self.close_invite(self.pairing.take_if(device));
    }

    /// Withdraw an open invite when it belongs to `device`, and say so.
    ///
    /// No epoch here: the caller is `forget`, which works with the tunnel
    /// down and holds no session of its own. What it knows is a device id,
    /// and an invite for a device that is being retired is one nobody should
    /// be able to spend — a code redeemed after the edge stopped holding its
    /// key hands the joining machine a credential that admits nowhere.
    pub(in crate::remote) fn withdraw_pairing_for(&self, device: &str) -> Option<String> {
        let _session = self.session();
        self.close_invite(self.pairing.take_if(device))
    }

    /// Reset everything session `epoch` owns — the invite, the `/mcp` grant,
    /// the paired flag, and the roster channel — unless a later session has
    /// taken the gateway over since. The request counters are history and
    /// stay.
    ///
    /// The guard is not defensive: a teardown takes its time. `take_down`
    /// drains for up to `DRAIN` before it gets here, and neither of its
    /// callers holds the `live` lock while it does — holding it would block
    /// `status` for the whole drain. So a `disable` and a fresh `enable` can
    /// overlap, and a teardown that cleared unconditionally would wipe the
    /// session that replaced it: the code the operator was just handed
    /// withdrawn, and an `/mcp` grant revoked without a word.
    ///
    /// Read and act under the one lock, which is the reason the epoch is not
    /// a bare atomic: an `enable` landing between a load and the clears
    /// would be wiped by a teardown that had just decided to leave it alone.
    ///
    /// **The epoch is counted up here too, not only in
    /// [`begin_session`](Self::begin_session).** Without that, an epoch whose
    /// session ended with no successor still matches, and every guard in this
    /// file that asks "is this still my session?" answers yes for a session
    /// that is gone. The reachable case is an `invite`: it reads the epoch
    /// under the serve slot, releases it, mints a key and writes two stores —
    /// and a `disable` landing in that window would leave `offer_pairing`
    /// holding an invite for a tunnel that is down. Ending a session has to
    /// retire its name along with its state.
    pub(in crate::remote) fn reset_session_if(&self, epoch: u64) {
        let mut session = self.session();
        if *session != epoch {
            return;
        }
        *session += 1;
        // Before the roster channel goes, below: a device that redeemed its
        // code before the listener closed, and that nothing has recorded yet,
        // is recorded on this session's writer.
        self.close_invite(self.pairing.take());
        self.mcp_allowed.store(false, Ordering::Relaxed);
        self.paired.store(false, Ordering::Relaxed);
        // Dropping the sender is what ends `roster_sync`: it reads until the
        // channel closes, so whatever the drain above put in the queue is
        // written first. Here rather than in `take_down` for the same reason
        // as everything else in this function — a superseded teardown would
        // otherwise silence the writer belonging to the session that
        // replaced it, and every label and `last_seen` after that would be
        // dropped on the floor with nothing to say so.
        self.notes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    /// Withdraw `open`, a no-op if it has ended, record how it ended, and say
    /// which device it was for.
    ///
    /// Withdrawn before it is read, and not the other way round: a device
    /// that redeems between a read that said "live" and the withdrawal would
    /// otherwise be taken out as withdrawn and never recorded. After the
    /// withdrawal the outcome cannot change.
    fn close_invite(&self, open: Option<Open>) -> Option<String> {
        let Open { device, invitation } = open?;
        invitation.withdraw();
        match invitation.ended() {
            Some(InviteOutcome::Redeemed { peer, label, .. }) => {
                let peer = peer.fingerprint();
                info!(device = %device, peer = %peer, "a device redeemed its invite");
                // The pairing request crossed the tunnel like any other, and
                // while the proxy answered it `status` counted it and named its
                // endpoint. The edge answers it now, so it is counted here, and
                // before `paired` flips: the pairing screen names the device
                // from the last peer the moment it reads `paired`.
                self.note_tunnelled_request(Some(&peer), None);
                self.paired.store(true, Ordering::Relaxed);
                self.note(Note::Joined {
                    device: device.clone(),
                    label,
                    peer: Some(peer.clone()),
                    at_ms: now_ms(),
                });
                self.emitter.emit(AppEvent::remote_paired(Some(peer)));
            }
            Some(InviteOutcome::Burned) => warn!(
                device = %device,
                "an invite was burned by wrong codes from more endpoints than the edge tracks: \
                 something holding this machine's ticket is guessing"
            ),
            // Expired or withdrawn, and whatever modelpipe adds: the row stays,
            // listed as never joined, which is the promise `docs/remote.md` makes.
            _ => {}
        }
        Some(device)
    }
}
