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
            .iter()
            .filter(|(_, q)| !q.is_empty())
            .map(|(model, q)| QueuedModelSnapshot {
                model_name: model.clone(),
                waiting: q.len(),
                oldest_wait_ms: q
                    .front()
                    .map_or(0, |w| now.duration_since(w.enqueued_at).as_millis() as u64),
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
                })
                .collect(),
            queued,
            total_queued: self.stats.total_queued,
            total_swaps: self.stats.total_swaps,
            secondary_slot: self.slot(PRIMARY_SLOT + 1).map_or_else(
                || self.secondary_slot.clone(),
                |r| SecondarySlotStatus::resident(&r.model_name),
            ),
        }
    }
}
