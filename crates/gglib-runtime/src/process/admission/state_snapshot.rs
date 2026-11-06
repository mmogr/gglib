//! [`QueueState::snapshot`]: the queue projected for the dashboard.
//!
//! A `#[path]` child of `state.rs`, so it reads the queue's private fields.

use tokio::time::Instant;

use gglib_core::domain::{
    AdmissionSnapshot, QueuedModelSnapshot, ResidentSlotSnapshot, SecondarySlotStatus,
    secondary_headroom,
};

use super::{PRIMARY_SLOT, QueueState, SlotState};

impl QueueState {
    /// Project the queue for the dashboard.
    ///
    /// `free_vram_bytes` is the machine's free VRAM as the caller read it,
    /// outside this lock; it sizes the second slot's "room for one more" and
    /// is wanted only while that slot is empty and no launch has been
    /// refused it, so a caller with nothing to say passes `None`.
    pub(in crate::process::admission) fn snapshot(
        &self,
        now: Instant,
        free_vram_bytes: Option<u64>,
    ) -> AdmissionSnapshot {
        let mut queued: Vec<QueuedModelSnapshot> = self
            .waiting
            .values()
            .filter_map(|q| q.front().map(|oldest| (oldest, q.len())))
            .map(|(oldest, waiting)| QueuedModelSnapshot {
                model_name: oldest.model_name.clone(),
                waiting,
                oldest_wait_ms: now.duration_since(oldest.enqueued_at).as_millis() as u64,
            })
            .collect();
        // Longest-waiting first: the entry a user needs to see is the one that
        // is about to force a swap.
        queued.sort_by_key(|q| std::cmp::Reverse(q.oldest_wait_ms));

        AdmissionSnapshot {
            slots: self
                .residents()
                .map(|(slot, r)| ResidentSlotSnapshot {
                    slot,
                    model_name: r.model_name.clone(),
                    model_id: r.model_id,
                    port: r.port,
                    inflight: r.inflight,
                    state: gglib_core::domain::ResidencyState::of_inflight(r.inflight),
                    is_primary: slot == PRIMARY_SLOT,
                    resident_for_secs: now.duration_since(r.resident_since).as_secs(),
                    runtime: r.runtime,
                    launch: Some(r.launch_facts()),
                })
                .collect(),
            queued,
            total_queued: self.stats.total_queued,
            total_swaps: self.stats.total_swaps,
            secondary_slot: self.secondary_slot_status(free_vram_bytes),
            generation: self.generation_snapshot(),
        }
    }

    /// Why the second slot is or is not in use: the model it holds, or the
    /// one loading into it; the refusal the last launch met while a model
    /// held the primary; else how much a second model may take now, when
    /// the free VRAM is known.
    fn secondary_slot_status(&self, free_vram_bytes: Option<u64>) -> SecondarySlotStatus {
        if let Some(r) = self.slot(PRIMARY_SLOT + 1) {
            return SecondarySlotStatus::resident(&r.model_name);
        }
        // A load into the slot beside a loaded primary says so; one into an
        // empty daemon (an image model's first load) is no second model yet.
        if matches!(self.slots[PRIMARY_SLOT + 1], SlotState::Loading { .. })
            && self.slot(PRIMARY_SLOT).is_some()
        {
            return SecondarySlotStatus::loading();
        }
        if self.second_slot_room_wanted() {
            return SecondarySlotStatus::available_with(secondary_headroom(free_vram_bytes));
        }
        self.secondary_slot.clone()
    }

    /// Whether the snapshot would say how much room the second slot has: a
    /// model is in the primary, the second slot is empty (nothing resident,
    /// nothing loading) and no launch has been refused it. The caller reads
    /// the free VRAM only then: with nothing loaded there is no "one more",
    /// and a load under way has the room.
    pub(in crate::process::admission) fn second_slot_room_wanted(&self) -> bool {
        self.slot(PRIMARY_SLOT).is_some()
            && matches!(self.slots[PRIMARY_SLOT + 1], SlotState::Empty)
            && self.secondary_slot == SecondarySlotStatus::default()
    }
}
