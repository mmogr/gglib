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

/// Log context-size resolution to stderr.
pub fn log_context_info(resolution: &ContextResolution) {
    match (&resolution.value, &resolution.source) {
        (Some(size), ContextResolutionSource::ExplicitFlag) => {
            eprintln!("  Context size: {} (explicit)", size);
        }
        (Some(size), ContextResolutionSource::ModelMetadata) => {
            eprintln!("  Context size: {} (from model metadata)", size);
        }
        (Some(size), ContextResolutionSource::SettingsDefault) => {
            eprintln!("  Context size: {} (from settings)", size);
        }
        (None, ContextResolutionSource::NotSpecified) => {
            eprintln!("  Context size: default (not specified)");
        }
        (None, ContextResolutionSource::MaxRequestedMissing) => {
            eprintln!("  Context size: max requested but not in metadata");
        }
        _ => {}
    }
}

/// Log mlock status to stderr.
pub fn log_mlock_info(mlock: bool) {
    if mlock {
        eprintln!("  Memory lock: enabled");
    }
}

/// Log resolved inference parameters to stderr.
pub fn log_inference_info(config: &InferenceConfig) {
    eprintln!("  Inference parameters:");
    if let Some(temp) = config.temperature {
        eprintln!("    Temperature: {}", temp);
    }
    if let Some(top_p) = config.top_p {
        eprintln!("    Top-p: {}", top_p);
    }
    if let Some(top_k) = config.top_k {
        eprintln!("    Top-k: {}", top_k);
    }
    if let Some(max_tokens) = config.max_tokens {
        eprintln!("    Max tokens: {}", max_tokens);
    }
    if let Some(repeat_penalty) = config.repeat_penalty {
        eprintln!("    Repeat penalty: {}", repeat_penalty);
    }
}

/// Log command execution at debug level.
pub fn log_command_execution(cmd: &std::process::Command) {
    tracing::debug!("Executing: {:?}", cmd);
}
