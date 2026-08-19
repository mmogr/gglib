//! Shared inference utilities.
//!
//! Functions used by `serve`, `chat`, and `question` handlers to resolve
//! inference parameters via the 3-level merge hierarchy and log diagnostics.

use anyhow::Result;

use crate::bootstrap::CliContext;
use gglib_core::Settings;
use gglib_core::domain::agent::DEFAULT_MAX_ITERATIONS;
use gglib_core::domain::{FieldSources, InferenceConfig};

/// Resolve inference parameters via the full merge hierarchy.
///
/// Merge order: CLI args (already in `config`) → per-model defaults, if
/// user-set → global defaults → per-model defaults, if auto-detected → the
/// class floor. Each layer fills in only `None` fields, except for the
/// parameters coupled to `temperature` — see
/// [`InferenceConfig::resolve_with_profile`] for both rules.
///
/// `gglib model explain <id>` prints the outcome of this resolution for any
/// model, naming the layer each parameter came from.
///
/// Returns the provenance alongside the values. Nothing in this crate could
/// previously say *why* a parameter ended up where it did — only `gglib model
/// explain` could, on a different code path — so a flag the coupling rule
/// discarded looked identical to one that was never passed.
pub(crate) async fn resolve_inference_config(
    ctx: &CliContext,
    config: InferenceConfig,
    model: &gglib_core::Model,
) -> Result<(InferenceConfig, FieldSources)> {
    let settings = ctx.app.settings().get().await?;
    let model_ctx = gglib_core::domain::ModelSamplingContext::for_model(model);
    Ok(config.resolve_with_profile_explained(
        None,
        model.inference_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
        model_ctx,
    ))
}

/// Resolve the maximum agent iterations via a 3-level fallback chain.
///
/// Merge order: CLI flag → persisted `Settings.max_tool_iterations` → `DEFAULT_MAX_ITERATIONS`.
/// This mirrors the pattern in [`resolve_inference_config`] and keeps handler code clean.
pub(crate) fn resolve_max_iterations(cli_override: Option<usize>, settings: &Settings) -> usize {
    cli_override
        .or_else(|| settings.max_tool_iterations.map(|v| v as usize))
        .unwrap_or(DEFAULT_MAX_ITERATIONS)
}

/// Log mlock status to stderr.
pub(crate) fn log_mlock_info(mlock: bool) {
    if mlock {
        eprintln!("  Memory lock: enabled");
    }
}

/// Log resolved inference parameters to stderr.
pub(crate) fn log_inference_info(config: &InferenceConfig) {
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
    if let Some(presence_penalty) = config.presence_penalty {
        eprintln!("    Presence penalty: {}", presence_penalty);
    }
    if let Some(min_p) = config.min_p {
        eprintln!("    Min-p: {}", min_p);
    }
}
