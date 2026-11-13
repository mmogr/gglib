//! The line a paired device's run waits in for an agent loop slot, when
//! every slot is taken.
//!
//! Only a device's run joins it ([`SlotLine::take_or_join`]); this machine's
//! own turn takes a free slot or is refused `agent_busy` at once
//! ([`SlotLine::take`]). The line is first come, first served: only its
//! first waiter asks the semaphore for a slot, so the slot that frees goes
//! to it, and the place each waiter reports is the one it is served in. No
//! one takes a free slot past the line while anyone waits in it. A waiter
//! leaves the line when it takes its slot or is dropped (its run cancelled,
//! its device forgotten, the daemon shutting down, or the bound reached),
//! and those behind it move up.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::http::StatusCode;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio::time::{Duration, Instant};

use super::run::coded;
use crate::error::HttpError;

/// How long a device's run waits in line before it ends `failed`
/// `agent_busy`.
pub(crate) const AGENT_SLOT_WAIT: Duration = Duration::from_secs(120);

/// How many device runs may wait at once. One that comes with the line
/// full is refused `agent_busy` at once.
pub(crate) const AGENT_SLOT_LINE: usize = 16;

/// What a turn is refused, or a run ends with, when it can have no slot.
pub(super) fn agent_busy() -> HttpError {
    coded(
        StatusCode::TOO_MANY_REQUESTS,
        "agent_busy",
        "all agent loop slots are in use; try again later",
    )
}

/// The waiters' tickets, first in line first, and the next ticket.
#[derive(Debug, Default)]
struct Queue {
    waiting: VecDeque<u64>,
    next: u64,
}

/// The line, over the agent semaphore's slots.
#[derive(Debug)]
pub(crate) struct SlotLine {
    slots: Arc<Semaphore>,
    queue: Mutex<Queue>,
    /// Told each time a waiter leaves, so those behind it look again.
    moved: watch::Sender<()>,
    /// How many may wait at once.
    cap: usize,
    /// How long a waiter waits, from joining, before it gives up.
    bound: Duration,
}

/// What a run is given: a slot now, or a place in line.
#[derive(Debug)]
pub(crate) enum Slot {
    Held(OwnedSemaphorePermit),
    Waiting(Place),
}

impl From<OwnedSemaphorePermit> for Slot {
    fn from(permit: OwnedSemaphorePermit) -> Self {
        Self::Held(permit)
    }
}

/// A place in line, and when it was taken. Dropped, it leaves the line.
#[derive(Debug)]
pub(crate) struct Place {
    line: Arc<SlotLine>,
    ticket: u64,
    joined: Instant,
}

impl SlotLine {
    pub(crate) fn new(slots: Arc<Semaphore>, cap: usize, bound: Duration) -> Self {
        Self {
            slots,
            queue: Mutex::new(Queue::default()),
            moved: watch::Sender::new(()),
            cap,
            bound,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A free slot, or `None` when every slot is taken or a device's run
    /// waits for one: a slot that frees goes to the line first.
    pub(crate) fn take(&self) -> Option<OwnedSemaphorePermit> {
        let queue = self.lock();
        if !queue.waiting.is_empty() {
            return None;
        }
        Arc::clone(&self.slots).try_acquire_owned().ok()
    }

    /// A free slot when no one waits; else a place at the back of the
    /// line; `None` when the line is full.
    pub(crate) fn take_or_join(self: &Arc<Self>) -> Option<Slot> {
        let mut queue = self.lock();
        if queue.waiting.is_empty()
            && let Ok(permit) = Arc::clone(&self.slots).try_acquire_owned()
        {
            return Some(Slot::Held(permit));
        }
        if queue.waiting.len() >= self.cap {
            return None;
        }
        let ticket = queue.next;
        queue.next += 1;
        queue.waiting.push_back(ticket);
        drop(queue);
        Some(Slot::Waiting(Place {
            line: Arc::clone(self),
            ticket,
            joined: Instant::now(),
        }))
    }
}

impl Place {
    /// This waiter's place in line, 1 being next.
    pub(crate) fn position(&self) -> u32 {
        let queue = self.line.lock();
        let ahead = queue.waiting.iter().take_while(|t| **t != self.ticket);
        u32::try_from(ahead.count() + 1).unwrap_or(u32::MAX)
    }

    /// When this waiter joined the line.
    pub(crate) const fn joined(&self) -> Instant {
        self.joined
    }

    /// When this waiter gives up: the line's bound after it joined.
    pub(crate) fn deadline(&self) -> Instant {
        self.joined + self.line.bound
    }

    /// Wait for this place to come first, then for a slot. Only the first
    /// waiter asks the semaphore, so the slots go in the line's order. The
    /// caller drops the place as soon as this answers, which moves the
    /// line up. `None` if the semaphore is closed.
    pub(crate) async fn slot(&self) -> Option<OwnedSemaphorePermit> {
        // Watched before the place is read, so no move is missed between.
        let mut moved = self.line.moved.subscribe();
        while self.position() > 1 {
            moved.changed().await.ok()?;
        }
        Arc::clone(&self.line.slots).acquire_owned().await.ok()
    }
}

impl Drop for Place {
    fn drop(&mut self) {
        self.line.lock().waiting.retain(|t| *t != self.ticket);
        self.line.moved.send_modify(|()| {});
    }
}

#[cfg(test)]
#[path = "slot_line_tests.rs"]
mod tests;
