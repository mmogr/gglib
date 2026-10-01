//! A hold: a run's claim on a resident it talks to directly, past the queue.
//!
//! An agent run's loop sends to llama-server's port itself, so it takes no
//! lease, and without one nothing stops a swap or a recycle from taking its
//! model mid-run. A hold is counted apart from `inflight`: the model stays
//! resident and unrecycled, but none of its `SERVER_PARALLEL` capacity is
//! taken, so proxy requests for it are still served beside the run. An
//! explicit stop, or the proxy's restart of a dead server, still takes a held
//! model ([`AdmissionQueue::evict`]).

use std::sync::Arc;

use gglib_core::ports::{AdmissionLease, AdmissionRelease, ModelRuntimeError};

use super::lease::AdmissionQueue;
use super::state::Resident;

/// Releases one hold on the resident it was taken on, when its lease drops.
#[derive(Debug)]
struct Hold {
    queue: Arc<AdmissionQueue>,
    port: u16,
    model_id: u32,
}

impl AdmissionRelease for Hold {
    fn release(&self, _slot: usize) {
        self.queue.lock().unhold(self.port, self.model_id);
        self.queue.notify();
    }
}

impl AdmissionQueue {
    /// Hold model `model_id`, listening on `port`, until the returned lease
    /// is dropped: it is neither swapped out nor recycled meanwhile. `None`
    /// when the resident on `port` is not that model, or there is none.
    pub fn hold(self: &Arc<Self>, port: u16, model_id: u32) -> Option<AdmissionLease> {
        let slot = self.lock().hold(port, model_id)?;
        let hold = Hold {
            queue: Arc::clone(self),
            port,
            model_id,
        };
        Some(AdmissionLease::new(Arc::new(hold), slot))
    }

    /// Empty `slot` for a recycle, as [`Self::evict`] does, unless a run
    /// holds its resident.
    ///
    /// # Errors
    ///
    /// A retryable [`ModelRuntimeError::AdmissionTimeout`] when it is held:
    /// the request that wanted the recycle is refused, and the model stays.
    pub fn evict_unheld(&self, slot: usize) -> Result<Option<Resident>, ModelRuntimeError> {
        let mut state = self.lock();
        if state.is_held(slot) {
            let model = state.slot(slot).map(|r| r.model_name.clone());
            return Err(ModelRuntimeError::AdmissionTimeout(format!(
                "'{}' is held by an agent run, so it is not recycled or relaunched at \
                 another context until the run ends; retry then",
                model.unwrap_or_default()
            )));
        }
        let previous = state.evict(slot);
        drop(state);
        self.notify();
        Ok(previous)
    }
}

#[cfg(test)]
#[path = "hold_tests.rs"]
mod tests;
