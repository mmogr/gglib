//! What a wait before a reply says: one sentence for an
//! [`AgentEvent::Waiting`], worded here once and carried on the frame as
//! its `said`, so every surface that shows a wait shows the same words.

use super::events::{AgentEvent, WaitingFor};

impl AgentEvent {
    /// A wait for `model`, by the name the catalogue shows, to load,
    /// `elapsed_ms` into the load, with the sentence it is shown as.
    #[must_use]
    pub fn model_load_wait(model: &str, elapsed_ms: u64) -> Self {
        Self::Waiting {
            reason: WaitingFor::ModelLoad,
            step: 0,
            total: 0,
            position: 0,
            model: Some(model.to_owned()),
            elapsed_ms: Some(elapsed_ms),
            said: Some(sentence(Some(model), Some(elapsed_ms))),
        }
    }

    /// The sentence a [`Waiting`](AgentEvent::Waiting) event is shown as;
    /// `None` for any other event. The frame's own `said` when it has one,
    /// else worded from its fields, as a frame logged before `said` was
    /// added is.
    ///
    /// A model being loaded: `Loading Qwen3-4B… 12 s`, by the name the
    /// catalogue shows and the whole seconds the load has taken, which are
    /// left out under one second; `Loading the model…` when the event names
    /// none.
    #[must_use]
    pub fn waiting_sentence(&self) -> Option<String> {
        let Self::Waiting {
            model,
            elapsed_ms,
            said,
            ..
        } = self
        else {
            return None;
        };
        Some(
            said.clone()
                .unwrap_or_else(|| sentence(model.as_deref(), *elapsed_ms)),
        )
    }
}

/// The words of a load's wait, from its fields.
fn sentence(model: Option<&str>, elapsed_ms: Option<u64>) -> String {
    let model = model.unwrap_or("the model");
    let seconds = elapsed_ms.map(|ms| ms / 1000).filter(|s| *s > 0);
    seconds.map_or_else(
        || format!("Loading {model}…"),
        |seconds| format!("Loading {model}… {seconds} s"),
    )
}

#[cfg(test)]
#[path = "waiting_tests.rs"]
mod tests;
