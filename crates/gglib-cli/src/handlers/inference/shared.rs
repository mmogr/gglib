//! Shared inference utilities.
//!
//! Functions used by `serve`, `chat`, and `question` handlers to resolve
//! inference parameters via the 3-level merge hierarchy and log diagnostics.

use anyhow::Result;

use crate::bootstrap::CliContext;
use gglib_core::Settings;
use gglib_core::domain::InferenceConfig;
use gglib_core::domain::agent::DEFAULT_MAX_ITERATIONS;
use gglib_runtime::llama::{ContextResolution, ContextResolutionSource};

/// Resolve inference parameters via the 3-level merge hierarchy.
///
/// Merge order: CLI args (already in `config`) → model defaults → global
/// defaults → hardcoded defaults. Each layer fills in only `None` fields.
pub async fn resolve_inference_config(
    ctx: &CliContext,
    config: InferenceConfig,
    model: &gglib_core::Model,
) -> Result<InferenceConfig> {
    let settings = ctx.app.settings().get().await?;

    Ok(InferenceConfig::resolve_with_hierarchy(
        Some(&config),
        model.inference_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
    ))
}

/// Resolve the maximum agent iterations via a 3-level fallback chain.
///
/// Merge order: CLI flag → persisted `Settings.max_tool_iterations` → `DEFAULT_MAX_ITERATIONS`.
/// This mirrors the pattern in [`resolve_inference_config`] and keeps handler code clean.
pub fn resolve_max_iterations(cli_override: Option<usize>, settings: &Settings) -> usize {
    cli_override
        .or_else(|| settings.max_tool_iterations.map(|v| v as usize))
        .unwrap_or(DEFAULT_MAX_ITERATIONS)
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
