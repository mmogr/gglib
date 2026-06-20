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
    /// Presence penalty — 0.0 = disabled, 1.5 = recommended for reasoning models
    /// (overrides model/global defaults)
    #[arg(long = "presence-penalty")]
    pub presence_penalty: Option<f32>,
    /// Min-P sampling threshold — 0.0 = disabled (overrides model/global defaults)
    #[arg(long = "min-p")]
    pub min_p: Option<f32>,
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
            presence_penalty: self.presence_penalty,
            min_p: self.min_p,
        }
    }
}

/// MTP (Multi-Token Prediction) speculative-decoding overrides for the `serve` command.
#[derive(Args, Debug, Clone, Default)]
pub struct MtpArgs {
    /// Number of MTP speculative draft tokens (auto-enabled when model has 'mtp' tag).
    ///
    /// Set to 0 to explicitly disable MTP even when the model supports it.
    #[arg(long)]
    pub mtp_draft_n_max: Option<u32>,
    /// Minimum acceptance probability for MTP draft tokens (default: 0.75).
    ///
    /// Only used when MTP is enabled. Lower values increase speed at the
    /// cost of output quality. Recommended range: 0.5–0.95.
    #[arg(long)]
    pub mtp_draft_p_min: Option<f32>,
}

/// Serve-command options that don't belong to another group.
#[derive(Args, Debug, Clone)]
pub struct ServeOptions {
    /// Force-enable Jinja template parsing for chat templates
    #[arg(long)]
    pub jinja: bool,
    /// Port to serve on
    #[arg(short, long, default_value = "8080")]
    pub port: u16,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            jinja: false,
            port: 8080,
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
    pub fn build(self) -> gglib_core::domain::chat::ConversationSettings {
        self.settings
    }
}
