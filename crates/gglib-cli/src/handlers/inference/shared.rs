//! Shared inference utilities.
//!
//! Functions used by `serve`, `chat`, and `question` handlers to resolve
//! inference parameters via the 3-level merge hierarchy and log diagnostics.

use anyhow::Result;

use crate::bootstrap::CliContext;
use gglib_core::domain::InferenceConfig;
use gglib_runtime::llama::{ContextResolution, ContextResolutionSource};

/// Resolve inference parameters via the 3-level merge hierarchy.
///
/// Merge order: CLI args (already in `config`) → model defaults → global
/// defaults → hardcoded defaults. Each layer fills in only `None` fields.
pub async fn resolve_inference_config(
    ctx: &CliContext,
    mut config: InferenceConfig,
    model: &gglib_core::Model,
) -> Result<InferenceConfig> {
    // Apply model defaults
    if let Some(ref model_defaults) = model.inference_defaults {
        config.merge_with(model_defaults);
    }

    // Apply global defaults
    let settings = ctx.app.settings().get().await?;
    if let Some(ref global_defaults) = settings.inference_defaults {
        config.merge_with(global_defaults);
    }

    // Apply hardcoded defaults
    config.merge_with(&InferenceConfig::with_hardcoded_defaults());

    Ok(config)
}

/// Log context-size resolution to stdout.
pub fn log_context_info(resolution: &ContextResolution) {
    match (&resolution.value, &resolution.source) {
        (Some(size), ContextResolutionSource::ExplicitFlag) => {
            println!("Context size: {} (explicit)", size);
        }
        (Some(size), ContextResolutionSource::ModelMetadata) => {
            println!("Context size: {} (from model metadata)", size);
        }
        (None, ContextResolutionSource::NotSpecified) => {
            println!("Context size: default (not specified)");
        }
        (None, ContextResolutionSource::MaxRequestedMissing) => {
            println!("Context size: max requested but not in metadata");
        }
        _ => {}
    }
}

/// Log mlock status to stdout.
pub fn log_mlock_info(mlock: bool) {
    if mlock {
        println!("Memory lock: enabled");
    }
}

/// Log resolved inference parameters to stdout.
pub fn log_inference_info(config: &InferenceConfig) {
    println!("Inference parameters:");
    if let Some(temp) = config.temperature {
        println!("  Temperature: {}", temp);
    }
    if let Some(top_p) = config.top_p {
        println!("  Top-p: {}", top_p);
    }
    if let Some(top_k) = config.top_k {
        println!("  Top-k: {}", top_k);
    }
    if let Some(max_tokens) = config.max_tokens {
        println!("  Max tokens: {}", max_tokens);
    }
    if let Some(repeat_penalty) = config.repeat_penalty {
        println!("  Repeat penalty: {}", repeat_penalty);
    }
}

/// Log command execution at debug level.
pub fn log_command_execution(cmd: &std::process::Command) {
    tracing::debug!("Executing: {:?}", cmd);
}
