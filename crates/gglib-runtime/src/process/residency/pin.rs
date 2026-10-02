//! The pin: the one model a [`ResidentSet`] will serve, when it is pinned.
//!
//! A child of `residency`, so it reads the set's private `pinned` field.

use gglib_core::ports::{ModelRuntimeError, PinnedSpec};

use super::ResidentSet;

impl ResidentSet {
    /// The model this set is pinned to, if any.
    ///
    /// The read side of [`Self::check_pinned`]: callers that want to avoid
    /// provoking a mismatch rather than handle one need to know the name up
    /// front.
    pub(in crate::process) fn pinned_name(&self) -> Option<String> {
        self.pinned
            .read()
            .ok()
            .and_then(|guard| guard.as_ref().map(|p| p.name.clone()))
    }

    /// Pin this set to one model, or clear the pin.
    pub(in crate::process) fn set_pin(&self, pin: Option<PinnedSpec>) {
        if let Ok(mut guard) = self.pinned.write() {
            *guard = pin;
        }
    }

    /// Reject a request for any model other than the pinned one.
    ///
    /// Checked before the queue is consulted, so a foreign request fails
    /// immediately rather than queueing behind — or worse, displacing — the
    /// pinned model.
    pub(super) fn check_pinned(&self, model_name: &str) -> Result<(), ModelRuntimeError> {
        match self.pinned_name() {
            Some(expected) if expected != model_name => {
                Err(ModelRuntimeError::PinnedModelMismatch {
                    expected,
                    requested: model_name.to_owned(),
                })
            }
            _ => Ok(()),
        }
    }
}
