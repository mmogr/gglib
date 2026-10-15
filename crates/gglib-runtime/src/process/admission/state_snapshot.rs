//! [`QueueState::snapshot`]: the queue projected for the dashboard.
//!
//! A `#[path]` child of `state.rs`, so it reads the queue's private fields.

use tokio::time::Instant;

use gglib_core::domain::{
    AdmissionSnapshot, QueuedModelSnapshot, ResidentSlotSnapshot, SecondarySlotStatus,
};

use super::{PRIMARY_SLOT, QueueState};

impl QueueState {
    /// Project the queue for the dashboard.
    pub(in crate::process::admission) fn snapshot(&self, now: Instant) -> AdmissionSnapshot {
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
                    is_primary: slot == PRIMARY_SLOT,
                    resident_for_secs: now.duration_since(r.resident_since).as_secs(),
                    runtime: r.runtime,
                })
                .collect(),
            queued,
            total_queued: self.stats.total_queued,
            total_swaps: self.stats.total_swaps,
            secondary_slot: self.slot(PRIMARY_SLOT + 1).map_or_else(
                || self.secondary_slot.clone(),
                |r| SecondarySlotStatus::resident(&r.model_name),
            ),
            generation: self.generation_snapshot(),
        }
    }
}
