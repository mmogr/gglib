//! Builds [`ConversationSettings`](gglib_core::domain::chat::ConversationSettings)
//! from the CLI argument groups.
//!
//! Lifted out of [`shared_args`](super::shared_args) whole: it is the one
//! thing in that module that is not an `#[derive(Args)]` group, and the module
//! is where the flag surface lives.

use crate::shared_args::{ContextArgs, SamplingArgs};

/// Builder for [`ConversationSettings`](gglib_core::domain::chat::ConversationSettings)
/// from CLI argument groups.
///
/// A single conversion point used by both `chat` and `q` handlers (DRY).
pub(crate) struct ConversationSettingsBuilder {
    settings: gglib_core::domain::chat::ConversationSettings,
}

impl ConversationSettingsBuilder {
    /// Start building settings from sampling and context args.
    pub(crate) fn new(sampling: &SamplingArgs, context: &ContextArgs) -> Self {
        Self {
            settings: gglib_core::domain::chat::ConversationSettings {
                temperature: sampling.temperature,
                top_p: sampling.top_p,
                top_k: sampling.top_k,
                max_tokens: sampling.max_tokens,
                repeat_penalty: sampling.repeat_penalty,
                ctx_size: context.ctx_size.clone(),
                mlock: if context.mlock { Some(true) } else { None },
                ..Default::default()
            },
        }
    }

    /// Set the model name used for this session.
    pub(crate) fn model_name(mut self, name: impl Into<String>) -> Self {
        self.settings.model_name = Some(name.into());
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
