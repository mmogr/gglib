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

    /// A paired device's run waiting for a free agent loop slot, at
    /// `position` in line (1 is next), `elapsed_ms` since it joined the
    /// line, with the sentence it is shown as.
    #[must_use]
    pub fn agent_slot_wait(position: u32, elapsed_ms: u64) -> Self {
        Self::Waiting {
            reason: WaitingFor::AgentSlot,
            step: 0,
            total: 0,
            position,
            model: None,
            elapsed_ms: Some(elapsed_ms),
            said: Some(slot_sentence(position, Some(elapsed_ms))),
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
    /// none. A run waiting for an agent slot: `Waiting for a free agent
    /// slot, 2 in line… 5 s`, by its place in line, seconds as for a load.
    #[must_use]
    pub fn waiting_sentence(&self) -> Option<String> {
        let Self::Waiting {
            reason,
            position,
            model,
            elapsed_ms,
            said,
            ..
        } = self
        else {
            return None;
        };
        Some(said.clone().unwrap_or_else(|| match reason {
            WaitingFor::ModelLoad => sentence(model.as_deref(), *elapsed_ms),
            WaitingFor::AgentSlot => slot_sentence(*position, *elapsed_ms),
        }))
    }
}

/// The words of a load's wait, from its fields.
fn sentence(model: Option<&str>, elapsed_ms: Option<u64>) -> String {
    let model = model.unwrap_or("the model");
    with_seconds(&format!("Loading {model}"), elapsed_ms)
}

/// The words of a wait for an agent slot, from its fields: its place in
/// line when it has one.
fn slot_sentence(position: u32, elapsed_ms: Option<u64>) -> String {
    let waiting = if position > 0 {
        format!("Waiting for a free agent slot, {position} in line")
    } else {
        "Waiting for a free agent slot".to_owned()
    };
    with_seconds(&waiting, elapsed_ms)
}

/// `what…`, then the whole seconds waited, left out under one second.
fn with_seconds(what: &str, elapsed_ms: Option<u64>) -> String {
    let seconds = elapsed_ms.map(|ms| ms / 1000).filter(|s| *s > 0);
    seconds.map_or_else(
        || format!("{what}…"),
        |seconds| format!("{what}… {seconds} s"),
    )
}

#[cfg(test)]
#[path = "waiting_tests.rs"]
mod tests;
