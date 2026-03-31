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
    /// Context size (use 'max' to auto-detect from model metadata)
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
