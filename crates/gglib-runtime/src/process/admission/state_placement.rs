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
//! evictable primary. When every slot it may use is held by a run it is
//! refused at once, because waiting would not help until that run ends.
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
//! for its place ([`QueueState::too_large_for_its_place`]) is refused at once
//! when a run holds the model in its way; otherwise it waits without leading
//! the line.

use gglib_core::domain::{
    RuntimeKind, SecondarySlotDecision, SlotFootprint, decide_secondary_slot_for,
};
use tokio::time::Instant;

use super::{AdmissionDecision, PRIMARY_SLOT, QueueState, Resident, SLOT_COUNT, SlotState, Ticket};

/// What a request asks the queue to place: the program that serves its
/// model, and the caller's verdict on the second slot.
///
/// Plain data, computed before the queue's lock is taken. A waiter keeps the
/// last one it polled with, so the queue can tell a waiter that only a held
/// slot could take from one that is merely early.
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

/// Why the queue refused a request outright instead of letting it wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// An image model could be placed only by evicting a model a run holds.
    HeldSlot {
        /// The held model in the way.
        held_model: String,
        /// What the image model needs, when the verdict knew it.
        needed_bytes: Option<u64>,
        /// The free memory the verdict was judged against, when it read one.
        free_bytes: Option<u64>,
    },
}

impl Refusal {
    /// The refusal in words, for a requester asking for `model`.
    #[must_use]
    pub fn describe(&self, model: &str) -> String {
        match self {
            Self::HeldSlot {
                held_model,
                needed_bytes,
                free_bytes,
            } => {
                let needed =
                    needed_bytes.map_or_else(String::new, |b| format!(", which needs {b} bytes"));
                let free = free_bytes.map_or_else(
                    || "free memory could not be read".to_owned(),
                    |b| format!("{b} bytes are free"),
                );
                format!(
                    "'{model}'{needed} does not fit beside '{held_model}' ({free}), and \
                     '{held_model}' is held by a run, so it cannot be swapped out; retry when \
                     the run ends"
                )
            }
        }
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
            if !self.too_large_for_its_place(candidate) {
                return slot == place;
            }
            // Too large for that place: it needs the memory of every other
            // loaded model too, so each is in its way. Only the hold check
            // reads these; `choose_image_slot` takes none of them.
            return slot != place && self.slot(slot).is_some();
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

    /// The held resident in `candidate`'s way, when every slot it may use is
    /// held by a run.
    ///
    /// `None` when any slot it may use is free of holds, even if that slot is
    /// busy or loading now: those end on their own, a hold ends only with its
    /// run.
    pub(super) fn held_only(&self, candidate: Candidate) -> Option<&Resident> {
        let mut held = None;
        for slot in (0..SLOT_COUNT).filter(|&slot| self.may_use(slot, candidate)) {
            if !self.is_held(slot) {
                return None;
            }
            held.get_or_insert(slot);
        }
        held.and_then(|slot| self.slot(slot))
    }

    /// Refuse an image model that only a held slot could take, forgetting its
    /// ticket. `None` for any other request.
    pub(super) fn refuse_held(
        &mut self,
        ticket: &Ticket,
        candidate: Candidate,
    ) -> Option<AdmissionDecision> {
        if candidate.runtime != RuntimeKind::StableDiffusion {
            return None;
        }
        let refusal = Refusal::HeldSlot {
            held_model: self.held_only(candidate)?.model_name.clone(),
            needed_bytes: candidate.secondary.footprint_bytes(),
            free_bytes: candidate.secondary.free_bytes(),
        };
        self.forget(ticket);
        Some(AdmissionDecision::Refuse(refusal))
    }

    /// Whether a waiting model can take its turn at the front of the line:
    /// false when its oldest waiter last polled with a candidate that only a
    /// held slot could take, or an image model too large for its place
    /// ([`Self::too_large_for_its_place`]). Such a waiter is passed over, so
    /// it never blocks the line for others and no resident stands aside for
    /// it; it keeps waiting until its own deadline.
    pub(super) fn may_lead(&self, candidate: Option<Candidate>) -> bool {
        candidate.is_none_or(|c| self.held_only(c).is_none() && !self.too_large_for_its_place(c))
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
        let beside = self
            .secondary_slots()
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
