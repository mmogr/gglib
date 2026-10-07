//! Shared inference utilities.
//!
//! Functions used by `serve`, `chat`, and `question` handlers to resolve
//! inference parameters via the 3-level merge hierarchy and log diagnostics.

use anyhow::Result;

use crate::bootstrap::CliContext;
use crate::presentation::sampling_values::stated_parameters;
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
/// Returns the provenance alongside the values, so a caller can say *why* a
/// parameter ended up where it did: without it, a flag the coupling rule
/// discarded looks identical to one that was never passed.
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

/// Log mlock status to stderr.
pub(crate) fn log_mlock_info(mlock: bool) {
    if mlock {
        eprintln!("  Memory lock: enabled");
    }
}

/// Log the sampling parameters the operator stated, to stderr.
///
/// From [`stated_parameters`], which reads the patch gglib puts on the wire
/// rather than naming fields: a banner that under-reports what it applies is
/// the same class of bug as one that over-reports it.
pub(crate) fn log_inference_info(config: &InferenceConfig) {
    let stated = stated_parameters(config);
    if stated.is_empty() {
        return;
    }

    eprintln!("  Inference parameters:");
    for (field, value) in stated {
        eprintln!("    {}: {value}", field.replace('_', "-"));
    }
}
