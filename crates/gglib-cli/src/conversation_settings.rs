//! Builds [`ConversationSettings`](gglib_core::domain::chat::ConversationSettings)
//! from the CLI argument groups, and holds `--max-tool-iterations` to the
//! ceiling a run takes ([`step_limit`]).
//!
//! Lifted out of [`shared_args`](super::shared_args) whole: it is the one
//! thing in that module that is not an `#[derive(Args)]` group, and the module
//! is where the flag surface lives.

use std::io;

use gglib_core::MAX_ITERATIONS_CEILING;

use crate::shared_args::ChatSamplingArgs;

/// The step limit a session runs with for the `--max-tool-iterations` it
/// was `asked`: the value asked, up to [`MAX_ITERATIONS_CEILING`]. A value
/// above the ceiling is not refused, so a script that passes one keeps
/// working; it runs with the ceiling, and one line on `said` names both.
/// `chat` and `q` each take the flag through here, once.
///
/// # Errors
///
/// The line could not be written to `said`.
pub(crate) fn step_limit(
    asked: Option<usize>,
    said: &mut impl io::Write,
) -> io::Result<Option<usize>> {
    match asked {
        Some(n) if n > MAX_ITERATIONS_CEILING => {
            writeln!(
                said,
                "--max-tool-iterations {n} is more than the {MAX_ITERATIONS_CEILING} steps a run \
                 may take; it runs with {MAX_ITERATIONS_CEILING}."
            )?;
            Ok(Some(MAX_ITERATIONS_CEILING))
        }
        _ => Ok(asked),
    }
}

/// Builder for [`ConversationSettings`](gglib_core::domain::chat::ConversationSettings)
/// from CLI argument groups.
///
/// A single conversion point used by both `chat` and `q` handlers (DRY).
pub(crate) struct ConversationSettingsBuilder {
    settings: gglib_core::domain::chat::ConversationSettings,
}

impl ConversationSettingsBuilder {
    /// Start building settings from the sampling flag a conversation keeps,
    /// `--max-tokens`.
    pub(crate) fn new(sampling: &ChatSamplingArgs) -> Self {
        Self {
            settings: gglib_core::domain::chat::ConversationSettings {
                max_tokens: sampling.max_tokens,
                ..Default::default()
            },
        }
    }

    /// Set the name the session's model is shown by.
    pub(crate) fn model_name(mut self, name: impl Into<String>) -> Self {
        self.settings.model_name = Some(name.into());
        self
    }

    /// Set the session's model, named by its machine, so a resume goes back
    /// to it. `None` for a model the catalogue did not hold.
    pub(crate) fn model(mut self, model: Option<gglib_core::domain::ModelRef>) -> Self {
        self.settings.model = model;
        self
    }

    /// Set what a turn's resolved model gives: its name, the model as its
    /// machine names it, and the profile the paired machine routed to.
    pub(crate) fn turn(self, turn: &crate::target::TurnModel) -> Self {
        self.model_name(&turn.name)
            .model(turn.model_ref.clone())
            .profile(turn.far_profile.clone())
    }

    /// Set the inference profile the session samples with, by name.
    pub(crate) fn profile(mut self, name: Option<String>) -> Self {
        self.settings.profile = name;
        self
    }

    /// Set tool-related configuration.
    pub(crate) fn tools(mut self, tools: Vec<String>, no_tools: bool) -> Self {
        self.settings.tools = tools;
        if no_tools {
            self.settings.no_tools = Some(true);
        }
        self
    }

    /// Set agent loop parameters.
    pub(crate) fn agent_params(
        mut self,
        max_iterations: Option<usize>,
        tool_timeout_ms: Option<u64>,
        max_parallel: Option<usize>,
    ) -> Self {
        self.settings.max_iterations = max_iterations;
        self.settings.tool_timeout_ms = tool_timeout_ms;
        self.settings.max_parallel = max_parallel;
        self
    }

    /// Consume the builder and return the finished settings.
    pub(crate) fn build(self) -> gglib_core::domain::chat::ConversationSettings {
        self.settings
    }
}

#[cfg(test)]
#[path = "conversation_settings_tests.rs"]
mod conversation_settings_tests;
