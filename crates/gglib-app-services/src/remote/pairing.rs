//! The invite a session has open: at most one, and how it ended.
//!
//! The code itself is modelpipe's. The tunnel edge mints it beside the
//! device's key, answers `POST /modelpipe/pair` itself, counts wrong codes per
//! endpoint and expires it, and none of that reaches this process. What is
//! left here is gglib's own rule, which modelpipe does not have: one invite at
//! a time per session, so a second `gglib remote invite` is refused while a
//! code is on screen rather than minting another beside it.
//!
//! Pure state behind a `std` mutex, never held across an await.

use std::num::NonZeroU8;
use std::sync::Mutex;
use std::time::Duration;

use modelpipe::InviteOutcome;

/// How long a code lives unused.
pub(crate) const PAIRING_TTL: Duration = Duration::from_mins(2);

/// How many wrong codes one endpoint may present before the edge locks it out
/// of the invite.
///
/// Three forgives a mistyped digit and is too few to guess with. The edge is
/// the only counter: a code is presented to it and nowhere else. A guesser
/// that mints fresh endpoints to get more tries is what
/// [`InviteOutcome::Burned`] reports.
pub(crate) const MAX_ATTEMPTS_AT_EDGE: NonZeroU8 = match NonZeroU8::new(3) {
    Some(bound) => bound,
    None => panic!("three is not zero"),
};

/// Whether an arming offers a pairing code at all.
///
/// A person running `gglib remote enable` is watching for one. A daemon
/// putting the tunnel back at startup is not, and a code nobody is watching
/// for is a live code nobody spends, for the two minutes it takes to expire,
/// at every boot. The distinction is a type rather than a `bool` so that the
/// silent path cannot be reached by forgetting an argument.
#[derive(Clone, Copy)]
pub(crate) enum Offer {
    /// Invite a device: mint its key and a code at the edge.
    Code,
    /// Arm the tunnel and nothing else.
    Silent,
}

/// What the gateway needs of an invite: how it ended, and a way to end it.
///
/// A trait rather than `modelpipe::InviteHandle` itself so that the gateway's
/// tests can hold an invite no listener minted, which is the reason
/// `teardown::Drain` exists too. `invite_watch.rs` implements it for the
/// handle.
pub(crate) trait Invitation: Send + Sync {
    /// How the invite ended, or `None` while its code is redeemable.
    fn ended(&self) -> Option<InviteOutcome>;
    /// End it now, as withdrawn. A no-op once it has ended.
    fn withdraw(&self);
}

/// An invite a session has open, and the device it was minted for.
pub(crate) struct Open {
    /// The name the edge holds the device's key under.
    pub(crate) device: String,
    pub(crate) invitation: Box<dyn Invitation>,
}

/// The invite a session currently has open, if any.
#[derive(Default)]
pub(crate) struct Pairing {
    open: Mutex<Option<Open>>,
}

impl Pairing {
    /// Hold `invitation` as the open invite.
    ///
    /// Replaces nothing on purpose: an invite already here may have ended as
    /// `Redeemed` and not been recorded yet, so the gateway takes it out and
    /// records it before calling this.
    pub(crate) fn begin(&self, device: String, invitation: Box<dyn Invitation>) {
        let mut slot = self.lock();
        debug_assert!(slot.is_none(), "an open invite was replaced unrecorded");
        *slot = Some(Open { device, invitation });
    }

    /// Whether a code is currently redeemable.
    ///
    /// **It reads and never clears.** An invite that ended as `Redeemed` stays
    /// here until whoever takes it out records the device. Clearing it here
    /// would lose that record to whichever `status` read came first.
    pub(crate) fn active(&self) -> bool {
        self.lock()
            .as_ref()
            .is_some_and(|open| open.invitation.ended().is_none())
    }

    /// Take the open invite out, live or ended.
    pub(crate) fn take(&self) -> Option<Open> {
        self.lock().take()
    }

    /// Take the open invite out if it was minted for `device`.
    ///
    /// An invite for some *other* device is none of the caller's business:
    /// retiring the laptop must not cancel the code a person is typing into
    /// their phone.
    pub(crate) fn take_if(&self, device: &str) -> Option<Open> {
        let mut slot = self.lock();
        if slot.as_ref().is_some_and(|open| open.device == device) {
            slot.take()
        } else {
            None
        }
    }

    // Nothing panics while holding the lock; recovering the guard is the
    // honest answer to an impossible poison.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Open>> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "pairing_tests.rs"]
pub(super) mod pairing_tests;
