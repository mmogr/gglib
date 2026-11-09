//! What a wait before a reply says: one sentence for an
//! [`AgentEvent::Waiting`], worded here once, so every surface that shows a
//! wait shows the same words.

use super::events::{AgentEvent, WaitingFor};

impl AgentEvent {
    /// The sentence a [`Waiting`](AgentEvent::Waiting) event is shown as;
    /// `None` for any other event.
    ///
    /// A model being loaded: `Loading Qwen3-4B… 12 s`, by the name the
    /// catalogue shows and the whole seconds the load has taken, which are
    /// left out under one second; `Loading the model…` when the event names
    /// none. An image render: `waiting for an image render, step 3 of 20`,
    /// or without the step before the render has reported one.
    #[must_use]
    pub fn waiting_sentence(&self) -> Option<String> {
        let Self::Waiting {
            reason,
            step,
            total,
            model,
            elapsed_ms,
            ..
        } = self
        else {
            return None;
        };
        Some(match reason {
            WaitingFor::ImageRender if *total > 0 => {
                format!("waiting for an image render, step {step} of {total}")
            }
            WaitingFor::ImageRender => "waiting for an image render".to_owned(),
            WaitingFor::ModelLoad => {
                let model = model.as_deref().unwrap_or("the model");
                let seconds = elapsed_ms.map(|ms| ms / 1000).filter(|s| *s > 0);
                seconds.map_or_else(
                    || format!("Loading {model}…"),
                    |seconds| format!("Loading {model}… {seconds} s"),
                )
            }
        })
    }
}

#[cfg(test)]
#[path = "waiting_tests.rs"]
mod tests;
