//! What a served engine is doing, as an app shows it, and the watch it is
//! read through.
//!
//! The runtime publishes seven states; an app shows five. A reply being
//! generated is [`EngineState::Ready`] still: the model is loaded at the
//! same context, and the reply on screen says it is being written. A model
//! being freed is [`EngineState::Idle`]: it is no longer served from the
//! moment it starts to go, so nothing can be asked of it. Two states that
//! show the same are one answer, so a reply's start and end answer nothing.

use std::sync::Arc;

use gglib_engine_llama as engine;
use tokio::sync::{Mutex, watch};

use crate::runtime::in_runtime;

/// What a served engine is doing. One model is held at most.
#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum EngineState {
    /// Serving, with no model loaded: none was asked for yet, or the one
    /// held is being freed.
    Idle,
    /// `model` is being loaded.
    Loading {
        /// Its id in the library, which tells it from another model of the
        /// same name.
        id: i64,
        /// The name the model is served under.
        model: String,
        /// How much of it has been read, from 0 to 1.
        fraction: f64,
    },
    /// `model` is loaded and served, generating a reply or not.
    Ready {
        /// Its id in the library, which tells it from another model of the
        /// same name.
        id: i64,
        /// The name the model is served under.
        model: String,
        /// The context it was loaded with, in tokens: the `context_size`
        /// each reply's usage reports.
        n_ctx: u64,
    },
    /// `model` could not be loaded. The next request naming a model tries
    /// again.
    Failed {
        /// Its id in the library, which tells it from another model of the
        /// same name.
        id: i64,
        /// The name the model is served under.
        model: String,
        /// Why, in gglib's words.
        reason: String,
    },
    /// The server was shut down: no model is held, and nothing listens.
    /// Nothing follows it.
    Stopped,
}

impl From<&engine::EngineState> for EngineState {
    fn from(state: &engine::EngineState) -> Self {
        match state {
            engine::EngineState::Idle | engine::EngineState::Unloading { .. } => Self::Idle,
            engine::EngineState::Loading {
                id,
                model,
                fraction,
            } => Self::Loading {
                id: i64::from(*id),
                model: model.clone(),
                fraction: f64::from(*fraction),
            },
            engine::EngineState::Ready { id, model, n_ctx }
            | engine::EngineState::Generating { id, model, n_ctx } => Self::Ready {
                id: i64::from(*id),
                model: model.clone(),
                n_ctx: u64::from(*n_ctx),
            },
            engine::EngineState::Failed { id, model, reason } => Self::Failed {
                id: i64::from(*id),
                model: model.clone(),
                reason: reason.clone(),
            },
            engine::EngineState::Stopped => Self::Stopped,
        }
    }
}

/// A served engine's state, followed a step at a time.
///
/// [`Self::next`] answers where the engine stands now, then again each time
/// that changes, and `None` once it has answered [`EngineState::Stopped`].
/// A watch only ever answers the latest state, so a step that came and went
/// between two calls is not answered. An app cannot cancel a call into this
/// library, so the watch is pulled; shutting the server down ends it.
#[derive(uniffi::Object)]
pub struct EngineWatch {
    /// Held by the one call answering.
    watching: Mutex<Watching>,
}

/// The engine's own watch, and what this one has answered from it.
struct Watching {
    states: watch::Receiver<engine::EngineState>,
    answered: Option<EngineState>,
}

impl EngineWatch {
    /// A watch over `states`, the runtime's watch of its engine.
    pub(crate) fn new(states: watch::Receiver<engine::EngineState>) -> Self {
        Self {
            watching: Mutex::new(Watching {
                states,
                answered: None,
            }),
        }
    }
}

#[uniffi::export]
impl EngineWatch {
    /// Where the engine stands, once that differs from the last answer, or
    /// `None` once [`EngineState::Stopped`] was answered. Calls answer one
    /// at a time.
    pub async fn next(self: Arc<Self>) -> Option<EngineState> {
        in_runtime(async move { self.watching.lock().await.next().await }).await
    }
}

impl Watching {
    async fn next(&mut self) -> Option<EngineState> {
        loop {
            if self.answered == Some(EngineState::Stopped) {
                return None;
            }
            let now = EngineState::from(&*self.states.borrow_and_update());
            if self.answered.as_ref() != Some(&now) {
                return Some(self.answer(now));
            }
            if self.states.changed().await.is_err() {
                // The engine has gone. It says `Stopped` before it goes; one
                // dropped without a shutdown is stopped all the same.
                return Some(self.answer(EngineState::Stopped));
            }
        }
    }

    fn answer(&mut self, state: EngineState) -> EngineState {
        self.answered = Some(state.clone());
        state
    }
}

#[cfg(test)]
#[path = "engine_watch_tests.rs"]
mod tests;
