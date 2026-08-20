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
    profile: Option<&gglib_core::domain::InferenceProfile>,
    model: &gglib_core::Model,
) -> Result<(InferenceConfig, FieldSources)> {
    let settings = ctx.app.settings().get().await?;
    let model_ctx = gglib_core::domain::ModelSamplingContext::for_model(model);
    Ok(config.resolve_with_profile_explained(
        profile.map(|selected| &selected.config),
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

/// Log the sampling parameters the operator stated, to stderr.
///
/// Reads [`InferenceConfig::to_openai_json_patch`] rather than naming fields,
/// because that patch *is* what gglib puts on the wire. A hand-written list
/// covered seven of the eighteen `SamplingArgs` can set, so
/// `--frequency-penalty 0.5` printed an empty "Inference parameters:" header
/// while still overriding every client on the endpoint — a banner that
/// under-reports what it applies is the same class of bug as one that
/// over-reports it.
pub(crate) fn log_inference_info(config: &InferenceConfig) {
    let patch = config.to_openai_json_patch();
    if patch.is_empty() {
        return;
    }

    eprintln!("  Inference parameters:");
    // Sorted so two runs of the same command print the same order.
    let mut fields: Vec<_> = patch.iter().collect();
    fields.sort_by_key(|(field, _)| *field);
    for (field, value) in fields {
        eprintln!("    {}: {value}", field.replace('_', "-"));
    }
}
