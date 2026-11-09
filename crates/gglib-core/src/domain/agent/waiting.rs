//! What a wait before a reply says: one sentence for an
//! [`AgentEvent::Waiting`], worded here once and carried on the frame as
//! its `said`, so every surface that shows a wait shows the same words.

use super::events::{AgentEvent, WaitingFor};

impl AgentEvent {
    /// A wait for `model`, by the name the catalogue shows, to load,
    /// `elapsed_ms` into the load, with the sentence it is shown as.
    #[must_use]
    pub fn model_load_wait(model: &str, elapsed_ms: u64) -> Self {
        Self::said(
            WaitingFor::ModelLoad,
            0,
            0,
            0,
            Some(model.to_owned()),
            Some(elapsed_ms),
        )
    }

    /// A wait behind an image render that is at `step` of `total` (0 of 0
    /// before it has reported one), this wait `position` in line, with the
    /// sentence it is shown as.
    #[must_use]
    pub fn image_render_wait(step: u32, total: u32, position: u32) -> Self {
        Self::said(WaitingFor::ImageRender, step, total, position, None, None)
    }

    fn said(
        reason: WaitingFor,
        step: u32,
        total: u32,
        position: u32,
        model: Option<String>,
        elapsed_ms: Option<u64>,
    ) -> Self {
        let said = sentence(reason, step, total, position, model.as_deref(), elapsed_ms);
        Self::Waiting {
            reason,
            step,
            total,
            position,
            model,
            elapsed_ms,
            said: Some(said),
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
    /// none. An image render: `Waiting for an image render, step 3 of 20`,
    /// without the step before the render has reported one, and with
    /// `, 2 in line` when the wait is not next.
    #[must_use]
    pub fn waiting_sentence(&self) -> Option<String> {
        let Self::Waiting {
            reason,
            step,
            total,
            position,
            model,
            elapsed_ms,
            said,
        } = self
        else {
            return None;
        };
        Some(said.clone().unwrap_or_else(|| {
            sentence(
                *reason,
                *step,
                *total,
                *position,
                model.as_deref(),
                *elapsed_ms,
            )
        }))
    }
}

/// The words of a wait, from its fields.
fn sentence(
    reason: WaitingFor,
    step: u32,
    total: u32,
    position: u32,
    model: Option<&str>,
    elapsed_ms: Option<u64>,
) -> String {
    match reason {
        WaitingFor::ImageRender => {
            let at = if total > 0 {
                format!(", step {step} of {total}")
            } else {
                String::new()
            };
            let place = if position > 1 {
                format!(", {position} in line")
            } else {
                String::new()
            };
            format!("Waiting for an image render{at}{place}")
        }
        WaitingFor::ModelLoad => {
            let model = model.unwrap_or("the model");
            let seconds = elapsed_ms.map(|ms| ms / 1000).filter(|s| *s > 0);
            seconds.map_or_else(
                || format!("Loading {model}…"),
                |seconds| format!("Loading {model}… {seconds} s"),
            )
        }
    }
}

#[cfg(test)]
#[path = "waiting_tests.rs"]
mod tests;
