//! Where a candidate may be placed, by the program that serves it.
//!
//! A `#[path]` child of `state.rs`, so it reads the queue's private fields.
//! Every function here is a pure question over the queue's state and a
//! [`Candidate`] the caller computed before the lock was taken; nothing the
//! caller supplies runs under it (see the locking notes in `lease.rs`).
//!
//! A model that chats is placed as it always was ([`QueueState::choose_slot`]).
//! An image model is placed by [`QueueState::choose_image_slot`]: it never
//! takes an empty primary, it takes the second slot when the memory check
//! grants it or when the primary is empty, and otherwise it swaps into an
//! evictable primary.
//!
//! One image model is loaded at a time. While another image model is in a
//! slot, loaded or still loading, that slot is the only one an image model
//! may use ([`QueueState::image_slot`]): it swaps in there once that model
//! is idle and waits while it renders or loads. It never loads beside it,
//! and never takes the chat model's slot to do so.
//!
//! A swap into the other image model's slot, or into the primary, is made
//! only when the newcomer fits in the memory the model it replaces gives back
//! ([`QueueState::fits_in_place_of`]); with nothing loaded beside the other
//! image model, the newcomer replaces it whatever the budget. One too large
//! for its place ([`QueueState::too_large_for_its_place`]) waits without
//! leading the line.

use gglib_core::domain::{
    RuntimeKind, SecondarySlotDecision, SlotFootprint, decide_secondary_slot_for,
};
use tokio::time::Instant;

use super::{PRIMARY_SLOT, QueueState, SLOT_COUNT, SlotState};

/// What a request asks the queue to place: the program that serves its
/// model, and the caller's verdict on the second slot.
///
/// Plain data, computed before the queue's lock is taken. A waiter keeps the
/// last one it polled with, so the queue can tell a waiter too large for its
/// place from one that is merely early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// The program that serves the model.
    pub runtime: RuntimeKind,
    /// Whether the model may take the second slot beside what is resident.
    pub secondary: SecondarySlotDecision,
}

impl Candidate {
    /// A model that chats, served by llama-server.
    #[must_use]
    pub const fn llama(secondary: SecondarySlotDecision) -> Self {
        Self {
            runtime: RuntimeKind::Llama,
            secondary,
        }
    }

    /// A model that draws, served by sd-server.
    #[must_use]
    pub const fn image(secondary: SecondarySlotDecision) -> Self {
        Self {
            runtime: RuntimeKind::StableDiffusion,
            secondary,
        }
    }
}

/// A bare second-slot verdict is a model that chats, which is what every
/// caller placed before image models existed.
impl From<SecondarySlotDecision> for Candidate {
    fn from(secondary: SecondarySlotDecision) -> Self {
        Self::llama(secondary)
    }
}

impl QueueState {
    /// Whether `candidate` may ever use `slot`, leaving aside whether the slot
    /// can be had right now.
    fn may_use(&self, slot: usize, candidate: Candidate) -> bool {
        // One image model at a time: the slot another one is in, and no
        // other.
        if candidate.runtime == RuntimeKind::StableDiffusion
            && let Some(place) = self.image_slot()
        {
            return slot == place;
        }
        let primary_empty = matches!(self.slots[PRIMARY_SLOT], SlotState::Empty);
        match (candidate.runtime, slot == PRIMARY_SLOT) {
            (RuntimeKind::Llama, true) => true,
            (RuntimeKind::Llama, false) => candidate.secondary.is_grant(),
            // Never an empty primary: an image model there would be the first
            // thing the next large chat model evicts.
            (RuntimeKind::StableDiffusion, true) => !primary_empty,
            // Beside the primary when the memory check grants it, and always
            // when the primary is empty: there is nothing to share memory
            // with, which also places a cold daemon whose budget is unknown.
            (RuntimeKind::StableDiffusion, false) => {
                candidate.secondary.is_grant() || primary_empty
            }
        }
    }

    /// Whether image model `candidate` fits in the memory `slot`'s resident
    /// gives back when it is swapped out.
    ///
    /// The verdict's free reading counts that resident as used, so it is
    /// judged again with the resident's weights added back. True on a grant,
    /// and when the verdict knows no footprint or no free reading, which
    /// places an unreadable budget as before. True for a slot with no
    /// resident: a model still loading gives nothing back yet, so the
    /// newcomer waits for it and is judged once it lands.
    fn fits_in_place_of(&self, slot: usize, candidate: Candidate) -> bool {
        let verdict = candidate.secondary;
        if verdict.is_grant() {
            return true;
        }
        let (Some(needed), Some(free), Some(outgoing)) = (
            verdict.footprint_bytes(),
            verdict.free_bytes(),
            self.slot(slot),
        ) else {
            return true;
        };
        decide_secondary_slot_for(
            RuntimeKind::StableDiffusion,
            Some(SlotFootprint {
                weights_bytes: needed,
                kv_bytes: 0,
            }),
            Some(free.saturating_add(outgoing.weights_bytes)),
        )
        .is_grant()
    }

    /// Whether image model `candidate` is too large for the only place it may
    /// take with what is loaded now, so no slot coming free would place it.
    ///
    /// While another image model is in a slot, that is the place, and it
    /// counts only while another model is loaded or loading beside it: alone,
    /// the newcomer is placed whatever the budget, as on a cold daemon.
    /// Otherwise the place is a primary that holds a model, unless the memory
    /// check grants the second slot. It waits for what is loaded to change,
    /// or for a reading that fits.
    fn too_large_for_its_place(&self, candidate: Candidate) -> bool {
        if candidate.runtime != RuntimeKind::StableDiffusion {
            return false;
        }
        let occupied = |slot: usize| !matches!(self.slots[slot], SlotState::Empty);
        let place = match self.image_slot() {
            Some(place) if (0..SLOT_COUNT).any(|slot| slot != place && occupied(slot)) => place,
            None if occupied(PRIMARY_SLOT) => PRIMARY_SLOT,
            Some(_) | None => return false,
        };
        !self.fits_in_place_of(place, candidate)
    }

    /// The slot an image model is in, loaded or still loading.
    ///
    /// `poll` places a request only when its own model is in no slot: it
    /// serves from a resident, and waits on a launch of the same model,
    /// first. So for an image model being placed this is another image
    /// model's slot.
    fn image_slot(&self) -> Option<usize> {
        (0..SLOT_COUNT)
            .find(|&slot| self.slots[slot].runtime() == Some(RuntimeKind::StableDiffusion))
    }

    /// Whether a waiting model can take its turn at the front of the line:
    /// false when its oldest waiter last polled as an image model too large
    /// for its place ([`Self::too_large_for_its_place`]). Such a waiter is
    /// passed over, so it never blocks the line for others and no resident
    /// stands aside for it; it keeps waiting until its own deadline.
    pub(super) fn may_lead(&self, candidate: Option<Candidate>) -> bool {
        candidate.is_none_or(|c| !self.too_large_for_its_place(c))
    }

    /// Pick the slot an image model launches into, if one can be had.
    ///
    /// 1. An empty secondary, when the memory check grants it or the primary
    ///    is empty.
    /// 2. Otherwise something has to go, which the fairness rules govern: an
    ///    evictable secondary on the same terms, then an evictable primary
    ///    that holds a model. An empty primary is never taken.
    ///
    /// While another image model is in a slot ([`Self::image_slot`]), that
    /// slot is the only one on offer. Step 1 then finds nothing and step 2
    /// takes that slot once it is evictable: a render in it, or its launch,
    /// is waited for, and the other slot's model is left alone.
    ///
    /// Nothing is chosen for an image model too large for its place
    /// ([`Self::too_large_for_its_place`]): loading it there would
    /// over-commit memory, and while another image model is loaded, loading
    /// it anywhere else would leave two loaded.
    pub(super) fn choose_image_slot(
        &self,
        model_id: u32,
        now: Instant,
        candidate: Candidate,
    ) -> Option<usize> {
        if self.too_large_for_its_place(candidate) {
            return None;
        }
        let beside = Self::secondary_slots()
            .filter(|&slot| self.may_use(slot, candidate))
            .collect::<Vec<_>>();
        if let Some(&slot) = beside
            .iter()
            .find(|&&slot| matches!(self.slots[slot], SlotState::Empty))
        {
            return Some(slot);
        }

        if !self.swap_is_permitted(model_id, now) {
            return None;
        }
        if let Some(&slot) = beside.iter().find(|&&slot| self.evictable(slot)) {
            return Some(slot);
        }
        (self.may_use(PRIMARY_SLOT, candidate) && self.evictable(PRIMARY_SLOT))
            .then_some(PRIMARY_SLOT)
    }
}
