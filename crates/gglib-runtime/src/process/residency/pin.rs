//! The pin: the one model a [`ResidentSet`] will serve, when it is pinned.
//!
//! A child of `residency`, so it reads the set's private `pinned` field.

use gglib_core::ports::{ModelLaunchSpec, ModelRuntimeError, PinnedSpec};

use super::ResidentSet;

impl ResidentSet {
    /// The model this set is pinned to, if any.
    ///
    /// The read side of [`Self::check_pinned`]: callers that want to avoid
    /// provoking a mismatch rather than handle one need to know the pin up
    /// front.
    pub(in crate::process) fn pinned(&self) -> Option<PinnedSpec> {
        self.pinned.read().ok().and_then(|guard| guard.clone())
    }

    /// Pin this set to one model, or clear the pin.
    pub(in crate::process) fn set_pin(&self, pin: Option<PinnedSpec>) {
        if let Ok(mut guard) = self.pinned.write() {
            *guard = pin;
        }
    }

    /// Reject a request whose model is not the pinned one.
    ///
    /// Takes the model the request resolved to and compares catalog ids, so a
    /// pinned endpoint answers to its own id as well as its name. Checked
    /// before the queue is consulted, so a foreign request fails immediately
    /// rather than queueing behind — or worse, displacing — the pinned model.
    pub(super) fn check_pinned(&self, spec: &ModelLaunchSpec) -> Result<(), ModelRuntimeError> {
        match self.pinned() {
            Some(pin) if pin.id != i64::from(spec.id) => {
                Err(ModelRuntimeError::PinnedModelMismatch {
                    expected: pin.name,
                    requested: spec.name.clone(),
                })
            }
            _ => Ok(()),
        }
    }
}
