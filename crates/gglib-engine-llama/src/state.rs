//! What the engine is doing, on a tokio watch.

use std::sync::{Mutex, PoisonError};

use tokio::sync::watch;

/// What the in-process engine is doing, as a phone shows it.
///
/// `id` is the model's id in the catalogue, and `model` the name it is
/// served under, which two models may share. Its only way out of
/// [`EngineState::Stopped`] is a new engine.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineState {
    /// No model is held.
    Idle,
    /// A model is being loaded.
    Loading {
        /// The id of the model being loaded.
        id: u32,
        /// The model being loaded.
        model: String,
        /// How much of it has been read, from 0 to 1, as the decoder
        /// reports it; a new value is sent at each whole percent.
        fraction: f32,
    },
    /// A model is held and no reply is being generated.
    Ready {
        /// The id of the model held.
        id: u32,
        /// The model held.
        model: String,
        /// The context it was loaded with, in tokens.
        n_ctx: u32,
    },
    /// A model is generating a reply.
    Generating {
        /// The id of the model generating.
        id: u32,
        /// The model generating.
        model: String,
        /// The context it was loaded with, in tokens, as its `Ready` said.
        n_ctx: u32,
    },
    /// A model is being freed: the reply in flight is ending, then its
    /// weights and context are released.
    Unloading {
        /// The id of the model being freed.
        id: u32,
        /// The model being freed.
        model: String,
    },
    /// A model failed to load. The next admission of a model tries again.
    Failed {
        /// The id of the model that failed.
        id: u32,
        /// The model that failed.
        model: String,
        /// What the decoder said, or that the load panicked.
        reason: String,
    },
    /// The engine was shut down: no model is held, its server no longer
    /// listens, and the watch has ended.
    Stopped,
}

/// The engine's [`EngineState`] and the watch it is published on.
///
/// Every change goes through here, so `Stopped` stays the last state sent:
/// once it is, nothing else is.
#[derive(Debug)]
pub(crate) struct States {
    /// `None` once stopped: dropping the sender is what ends the watch.
    tx: Mutex<Option<watch::Sender<EngineState>>>,
    /// Kept so a watcher can still subscribe once the sender is gone.
    rx: watch::Receiver<EngineState>,
    /// Every state sent, in order: a watch shows only the latest, and a
    /// test reads the whole sequence.
    #[cfg(test)]
    sent: Mutex<Vec<EngineState>>,
}

/// Says the load of `model` failed if it is dropped by a panic, so a load
/// that panics leaves the watch on `Failed`, not on `Loading`.
#[derive(Debug)]
pub(crate) struct FailedOnPanic<'a> {
    states: &'a States,
    id: u32,
    model: &'a str,
}

impl Drop for FailedOnPanic<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.states.set(EngineState::Failed {
                id: self.id,
                model: self.model.to_owned(),
                reason: "the load panicked".to_owned(),
            });
        }
    }
}

impl States {
    /// An engine holding nothing.
    pub(crate) fn new() -> Self {
        let (tx, rx) = watch::channel(EngineState::Idle);
        Self {
            tx: Mutex::new(Some(tx)),
            rx,
            #[cfg(test)]
            sent: Mutex::new(Vec::new()),
        }
    }

    /// A guard, held for the whole of a load of model `id`, served as
    /// `model`, that moves to `Failed` if the load panics.
    pub(crate) fn failed_on_panic<'a>(&'a self, id: u32, model: &'a str) -> FailedOnPanic<'a> {
        FailedOnPanic {
            states: self,
            id,
            model,
        }
    }

    /// A receiver whose current value is the state now, and which is told
    /// each change after it.
    pub(crate) fn subscribe(&self) -> watch::Receiver<EngineState> {
        let mut rx = self.rx.clone();
        rx.mark_unchanged();
        rx
    }

    /// The state now.
    pub(crate) fn current(&self) -> EngineState {
        self.rx.borrow().clone()
    }

    /// Move to `next`.
    pub(crate) fn set(&self, next: EngineState) {
        self.modify(|state| {
            *state = next;
            true
        });
    }

    /// Model `id`, served as `model`, has read `fraction` of itself:
    /// `Loading` with it, sent at the load's first report and at each whole
    /// percent past the one shown.
    pub(crate) fn loading(&self, id: u32, model: &str, fraction: f32) {
        let fraction = fraction.clamp(0.0, 1.0);
        self.modify(|state| {
            let shown = match state {
                EngineState::Loading {
                    id: loading,
                    fraction: shown,
                    ..
                } if *loading == id => Some(whole_percent(*shown)),
                _ => None,
            };
            if shown.is_some_and(|shown| whole_percent(fraction) <= shown) {
                return false;
            }
            *state = EngineState::Loading {
                id,
                model: model.to_owned(),
                fraction,
            };
            true
        });
    }

    /// Model `id` has started a reply: `Ready` becomes `Generating`, with
    /// the same context. Any other state is left alone, since the model is
    /// on its way out.
    pub(crate) fn generating(&self, id: u32) {
        self.modify(|state| match state {
            EngineState::Ready {
                id: held,
                model,
                n_ctx,
            } if *held == id => {
                *state = EngineState::Generating {
                    id,
                    model: model.clone(),
                    n_ctx: *n_ctx,
                };
                true
            }
            _ => false,
        });
    }

    /// Model `id`'s reply has ended: `Generating` becomes `Ready` again.
    pub(crate) fn generated(&self, id: u32, n_ctx: u32) {
        self.modify(|state| match state {
            EngineState::Generating {
                id: held, model, ..
            } if *held == id => {
                *state = EngineState::Ready {
                    id,
                    model: model.clone(),
                    n_ctx,
                };
                true
            }
            _ => false,
        });
    }

    /// Send `Stopped` and end the watch. Idempotent.
    pub(crate) fn stop(&self) {
        let tx = self
            .tx
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(tx) = tx {
            tx.send_replace(EngineState::Stopped);
            #[cfg(test)]
            self.note(EngineState::Stopped);
        }
    }

    fn modify(&self, f: impl FnOnce(&mut EngineState) -> bool) {
        let tx = self.tx.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(tx) = tx.as_ref()
            && tx.send_if_modified(f)
        {
            #[cfg(test)]
            self.note(tx.borrow().clone());
        }
    }

    /// Every state sent so far, in order.
    #[cfg(test)]
    pub(crate) fn sent(&self) -> Vec<EngineState> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    #[cfg(test)]
    fn note(&self, state: EngineState) {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(state);
    }
}

/// `fraction`, from 0 to 1, in whole percent.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a fraction clamped to 0..=1 is 0..=100 percent once floored"
)]
fn whole_percent(fraction: f32) -> u8 {
    (fraction * 100.0).floor() as u8
}
