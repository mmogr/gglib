//! What the engine is doing, on a tokio watch.

use std::sync::{Mutex, PoisonError};

use tokio::sync::watch;

/// What the in-process engine is doing, as a phone shows it.
///
/// `model` is the name the model is served under. Its only way out of
/// [`EngineState::Stopped`] is a new engine.
#[derive(Debug, Clone, PartialEq)]
pub enum EngineState {
    /// No model is held.
    Idle,
    /// A model is being loaded.
    Loading {
        /// The model being loaded.
        model: String,
        /// How much of it has been read, from 0 to 1, as the decoder
        /// reports it; a new value is sent at each whole percent.
        fraction: f32,
    },
    /// A model is held and no reply is being generated.
    Ready {
        /// The model held.
        model: String,
        /// The context it was loaded with, in tokens.
        n_ctx: u32,
    },
    /// A model is generating a reply.
    Generating {
        /// The model generating.
        model: String,
        /// The context it was loaded with, in tokens, as its `Ready` said.
        n_ctx: u32,
    },
    /// A model is being freed: the reply in flight is ending, then its
    /// weights and context are released.
    Unloading {
        /// The model being freed.
        model: String,
    },
    /// A model failed to load. The next admission of a model tries again.
    Failed {
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
    model: &'a str,
}

impl Drop for FailedOnPanic<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.states.set(EngineState::Failed {
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

    /// A guard, held for the whole of a load of `model`, that moves to
    /// `Failed` if the load panics.
    pub(crate) fn failed_on_panic<'a>(&'a self, model: &'a str) -> FailedOnPanic<'a> {
        FailedOnPanic {
            states: self,
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

    /// `model` has read `fraction` of itself: `Loading` with it, sent at the
    /// load's first report and at each whole percent past the one shown.
    pub(crate) fn loading(&self, model: &str, fraction: f32) {
        let fraction = fraction.clamp(0.0, 1.0);
        self.modify(|state| {
            let shown = match state {
                EngineState::Loading {
                    model: loading,
                    fraction: shown,
                } if loading == model => Some(whole_percent(*shown)),
                _ => None,
            };
            if shown.is_some_and(|shown| whole_percent(fraction) <= shown) {
                return false;
            }
            *state = EngineState::Loading {
                model: model.to_owned(),
                fraction,
            };
            true
        });
    }

    /// `model` has started a reply: `Ready` becomes `Generating`, with the
    /// same context. Any other state is left alone, since the model is on
    /// its way out.
    pub(crate) fn generating(&self, model: &str) {
        self.modify(|state| match state {
            EngineState::Ready { model: held, n_ctx } if held == model => {
                *state = EngineState::Generating {
                    model: model.to_owned(),
                    n_ctx: *n_ctx,
                };
                true
            }
            _ => false,
        });
    }

    /// `model`'s reply has ended: `Generating` becomes `Ready` again.
    pub(crate) fn generated(&self, model: &str, n_ctx: u32) {
        self.modify(|state| match state {
            EngineState::Generating { model: held, .. } if held == model => {
                *state = EngineState::Ready {
                    model: model.to_owned(),
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
