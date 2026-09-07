//! The one-at-a-time slot each side of the tunnel occupies.
//!
//! `enable` and `connect` are the same shape: there may be exactly one, and
//! building it is slow — a dial that waits out an unreachable peer, or a
//! listener that waits ten seconds for a relay. Both held their mutex across
//! that, and `status` reads both. `tokio::sync::Mutex` is FIFO-fair, so a
//! reader arriving during the slow part waits for all of it: `gglib remote
//! status` gives the daemon five seconds and a dial can hold the lock for
//! far longer, and `gglib remote disconnect` — the one command that exists
//! to end a hanging connect — queued behind the connect it was cancelling.
//!
//! So the lock is held for the transitions and nothing else. A caller
//! reserves the slot, drops the lock, does the slow work, and comes back to
//! install what it built. Because a teardown may have taken the slot while
//! it was away, every reservation carries two things: a generation, which is
//! how the caller finds out it no longer owns the slot, and a cancellation
//! token, which is how the teardown stops the work instead of orphaning it.

use tokio_util::sync::CancellationToken;

/// A slot: empty, being filled, or full.
pub(super) enum Slot<T> {
    /// Nothing here, and nothing on the way.
    Empty,
    /// Reserved by a call that is still building what goes in it.
    Filling {
        /// Which call reserved it. A teardown empties the slot without
        /// waiting, so this is what tells that call, when it returns, that
        /// what it built is no longer wanted.
        generation: u64,
        /// Cancelled when the slot is taken away, so the slow work can stop
        /// rather than run to completion for nobody.
        cancel: CancellationToken,
    },
    /// Occupied.
    Full(T),
}

/// Why a slot could not be reserved.
///
/// Two cases rather than one because they are two different sentences to
/// the person who typed the command: something is already up, or something
/// is on its way up and they asked twice.
#[derive(Debug)]
pub(super) enum Busy {
    /// Another call is building the thing that goes in it.
    Filling,
    /// Something is already in it.
    Full,
}

/// What emptying a slot found.
pub(super) enum Taken<T> {
    /// Nothing was there.
    Empty,
    /// A reservation was, and has now been cancelled. Whoever holds it
    /// finds the slot gone and takes its own work down; there is nothing
    /// here to drain, because nothing was ever installed.
    Cancelled,
    /// The thing itself.
    Value(T),
}

impl<T> Slot<T> {
    /// What holds the slot, if anything.
    ///
    /// For the cheap refusal a caller makes *before* doing any work, so
    /// `connect` while connected still says "already connected" rather than
    /// reporting the first thing it happens to find wrong with the
    /// arguments. Not a reservation: the lock is gone the moment this
    /// returns, and two callers can both pass it. [`Self::reserve`] is
    /// where exactly one of them wins.
    pub(super) const fn busy(&self) -> Option<Busy> {
        match self {
            Self::Empty => None,
            Self::Filling { .. } => Some(Busy::Filling),
            Self::Full(_) => Some(Busy::Full),
        }
    }

    /// Take the slot for `generation`, handing back the token a teardown
    /// will cancel.
    pub(super) fn reserve(&mut self, generation: u64) -> Result<CancellationToken, Busy> {
        if let Some(busy) = self.busy() {
            return Err(busy);
        }
        let cancel = CancellationToken::new();
        *self = Self::Filling {
            generation,
            cancel: cancel.clone(),
        };
        Ok(cancel)
    }

    /// Put `value` in, and say whether this reservation still owned the
    /// slot. `false` means a teardown took it: the caller's job is then to
    /// throw away what it built, not to install it.
    pub(super) fn install(&mut self, generation: u64, value: T) -> bool {
        if !matches!(self, Self::Filling { generation: g, .. } if *g == generation) {
            return false;
        }
        *self = Self::Full(value);
        true
    }

    /// Give the slot back after a failure. A no-op unless this reservation
    /// still holds it — a teardown that already took it has moved on, and
    /// emptying the slot again could only wipe out a later reservation.
    pub(super) fn release(&mut self, generation: u64) {
        if matches!(self, Self::Filling { generation: g, .. } if *g == generation) {
            *self = Self::Empty;
        }
    }

    /// What is in the slot, when it is full.
    ///
    /// A reservation reads as nothing: a tunnel that is still binding is
    /// not one anything can be told about yet.
    pub(super) const fn full(&self) -> Option<&T> {
        match self {
            Self::Full(value) => Some(value),
            _ => None,
        }
    }

    /// Empty the slot, saying what was in it and cancelling a reservation
    /// on the way out.
    pub(super) fn take(&mut self) -> Taken<T> {
        match std::mem::replace(self, Self::Empty) {
            Self::Empty => Taken::Empty,
            Self::Filling { cancel, .. } => {
                cancel.cancel();
                Taken::Cancelled
            }
            Self::Full(value) => Taken::Value(value),
        }
    }

    /// Empty the slot only when what is in it is the caller's.
    ///
    /// What a watcher that outlived its connection needs: it must not take
    /// down the one that replaced it, and it must not disturb a dial that
    /// is on its way to replacing it either.
    pub(super) fn take_if(&mut self, is_mine: impl FnOnce(&T) -> bool) -> Option<T> {
        if !self.full().is_some_and(is_mine) {
            return None;
        }
        match self.take() {
            Taken::Value(value) => Some(value),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "slot_tests.rs"]
mod slot_tests;
