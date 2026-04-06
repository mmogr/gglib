//! Shared CLI argument groups.
//!
//! Reusable `#[derive(Args)]` structs that are flattened into multiple commands
//! via `#[command(flatten)]`, eliminating duplicate field definitions across
//! `Serve`, `Chat`, and `Question`.

use clap::Args;

/// Sampling-parameter overrides common to all inference commands.
///
/// Each field is optional. When `None`, the 3-level merge hierarchy
/// (CLI → model defaults → global defaults → hardcoded) fills in the value.
#[derive(Args, Debug, Clone, Default)]
pub struct SamplingArgs {
    /// Temperature for sampling (0.0-2.0, overrides model/global defaults)
    #[arg(long)]
    pub temperature: Option<f32>,
    /// Top-p sampling (0.0-1.0, overrides model/global defaults)
    #[arg(long = "top-p")]
    pub top_p: Option<f32>,
    /// Top-k sampling (overrides model/global defaults)
    #[arg(long = "top-k")]
    pub top_k: Option<i32>,
    /// Maximum tokens to generate (overrides model/global defaults)
    #[arg(long = "max-tokens")]
    pub max_tokens: Option<u32>,
    /// Repeat penalty (overrides model/global defaults)
    #[arg(long = "repeat-penalty")]
    pub repeat_penalty: Option<f32>,
}

/// Context-size and memory-lock flags common to all inference commands.
#[derive(Args, Debug, Clone, Default)]
pub struct ContextArgs {
    /// Context size override (number or 'max' for model metadata).
    /// Falls back to the global default from 'gglib config settings'.
    #[arg(short, long)]
    pub ctx_size: Option<String>,
    /// Enable memory lock
    #[arg(long)]
    pub mlock: bool,
}

impl SamplingArgs {
    /// Convert into an [`InferenceConfig`](gglib_core::domain::InferenceConfig).
    pub fn into_inference_config(self) -> gglib_core::domain::InferenceConfig {
        gglib_core::domain::InferenceConfig {
            temperature: self.temperature,
            top_p: self.top_p,
            top_k: self.top_k,
            max_tokens: self.max_tokens,
            repeat_penalty: self.repeat_penalty,
        }
    }
}

/// Builder for [`ConversationSettings`](gglib_core::domain::chat::ConversationSettings)
/// from CLI argument groups.
///
/// A single conversion point used by both `chat` and `q` handlers (DRY).
pub struct ConversationSettingsBuilder {
    settings: gglib_core::domain::chat::ConversationSettings,
}

impl ConversationSettingsBuilder {
    /// Start building settings from sampling and context args.
    pub fn new(sampling: &SamplingArgs, context: &ContextArgs) -> Self {
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
    pub fn model_name(mut self, name: impl Into<String>) -> Self {
        self.settings.model_name = Some(name.into());
        self
    }

    /// Set tool-related configuration.
    pub fn tools(mut self, tools: Vec<String>, no_tools: bool) -> Self {
        self.settings.tools = tools;
        if no_tools {
            self.settings.no_tools = Some(true);
        }
        self
    }

    /// Set agent loop parameters.
    pub fn agent_params(
        mut self,
        max_iterations: usize,
        tool_timeout_ms: Option<u64>,
        max_parallel: Option<usize>,
    ) -> Self {
        self.settings.max_iterations = Some(max_iterations);
        self.settings.tool_timeout_ms = tool_timeout_ms;
        self.settings.max_parallel = max_parallel;
        self
    }

    /// Consume the builder and return the finished settings.
    pub fn build(self) -> gglib_core::domain::chat::ConversationSettings {
        self.settings
    }
}
