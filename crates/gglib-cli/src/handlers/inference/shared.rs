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
        eprintln!("    {}: {}", field.replace('_', "-"), render(value));
    }
}

/// Render one patch value the way the user typed it.
///
/// Every sampling parameter gglib models as a float is an `f32`, and the patch
/// carries them as JSON numbers — i.e. `f64`. Printing that directly shows
/// `0.1` as `0.10000000149011612`: the f64 nearest to the f32 nearest to 0.1,
/// which is accurate, useless, and not what anyone typed. Narrowing back to
/// `f32` before formatting restores the shortest representation that
/// round-trips, so `--temperature 0.1` prints `0.1`.
fn render(value: &serde_json::Value) -> String {
    match value.as_f64() {
        // Integral values print without a synthetic ".0" — `max-tokens: 512`,
        // not `512.0`.
        Some(n) if n.fract() == 0.0 => format!("{n}"),
        Some(n) => format!("{}", n as f32),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The regression: an `f32` widened through JSON printed its f64 shadow.
    #[test]
    fn a_float_prints_as_the_user_typed_it() {
        let patch = InferenceConfig {
            temperature: Some(0.1),
            top_p: Some(0.95),
            ..Default::default()
        }
        .to_openai_json_patch();

        assert_eq!(render(&patch["temperature"]), "0.1");
        assert_eq!(render(&patch["top_p"]), "0.95");
    }

    /// Counts stay counts: no synthetic decimal point.
    #[test]
    fn an_integral_value_prints_without_a_fraction() {
        let patch = InferenceConfig {
            max_tokens: Some(512),
            top_k: Some(40),
            ..Default::default()
        }
        .to_openai_json_patch();

        assert_eq!(render(&patch["max_tokens"]), "512");
        assert_eq!(render(&patch["top_k"]), "40");
    }

    /// Non-numeric fields (the reasoning effort level) pass through unharmed.
    #[test]
    fn a_non_numeric_value_falls_back_to_its_own_rendering() {
        assert_eq!(render(&serde_json::json!("high")), "\"high\"");
    }
}
