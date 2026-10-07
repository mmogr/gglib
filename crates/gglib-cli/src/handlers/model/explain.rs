//! `gglib model explain` handler.
//!
//! Resolves a model's sampling parameters through the same hierarchy the
//! proxy uses and prints each one beside the layer that supplied it.
//!
//! The resolution is [`ModelOps::explain_sampling`], the call `GET
//! /api/models/{id}/explain` answers with, so a terminal and the inspector's
//! Sampling section are told the same thing. Nothing here re-implements the
//! ladder or looks a profile up, so this command cannot describe a hierarchy
//! that differs from the one that runs.

use anyhow::{Result, anyhow};
use gglib_app_services::{GuiError, ModelOps, SamplingExplanationDto};
use gglib_core::Settings;
use gglib_core::domain::{FitInputs, Model};
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size_with_source};
use gglib_runtime::llama::args::resolve_kv_cache_types;
use gglib_runtime::ports_impl::model_shards::resident_bytes;
use gglib_runtime::process::residency::explain::explain_fit;

use super::resolver;
use crate::bootstrap::CliContext;
use crate::presentation::explain_display;

/// Execute `gglib model explain <id> [--profile NAME]`.
pub(crate) async fn execute(
    ctx: &CliContext,
    ops: &ModelOps,
    identifier: &str,
    profile: Option<&str>,
) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    let explanation = explain(ops, model.id, profile).await?;
    let settings = ctx.app.settings().get().await?;

    explain_display::print_explanation(&model.name, model.id, &explanation);
    print_context_explanation(&model, &settings);

    Ok(())
}

/// The explanation of model `id`'s stored sampling, with `profile` applied
/// when one is named.
///
/// A name that is no configured profile is an error rather than a fall back
/// to no profile: someone who passed `--profile` wants to see that profile's
/// effect. Its message is the whole answer, so it is printed as it is.
async fn explain(ops: &ModelOps, id: i64, profile: Option<&str>) -> Result<SamplingExplanationDto> {
    let explained = ops.explain_sampling(id, profile).await;
    explained.map_err(|refused| match refused {
        GuiError::ValidationFailed(message) => anyhow!(message),
        other => other.into(),
    })
}

/// Print the context chain, and what the fit worked from where it reached one.
///
/// The two constants behind a fitted context — `BUDGET_UTILISATION` and the
/// co-resident reservation — are judgement calls, and ADR 0009 says so. The
/// only way they stop being guesses is if the numbers they produce are visible
/// when somebody looks, and this is where they are. ADR 0009's first kill
/// criterion needs exactly this reading across a catalog: if the chosen rung
/// is routinely far below `unsnapped`, the ladder is too coarse.
///
/// Every value comes from [`gglib_runtime::process::residency::explain::explain_fit`]
/// and [`resolve_context_size_with_source`] — the same calls a launch makes —
/// so this cannot describe a chain that differs from the one that runs.
fn print_context_explanation(model: &Model, settings: &Settings) {
    let (fitted, inputs) = context_fit(model);

    let (resolved, source) = resolve_context_size_with_source(&ServerConfigOptions {
        model_server_ctx: model
            .server_defaults
            .as_ref()
            .and_then(|s| s.context_length),
        global_default_ctx: settings.default_context_size,
        fitted_ctx: fitted,
        ..Default::default()
    });

    println!();
    println!("Context");
    println!("  {:<22} {resolved} ({})", "serves at", source.label());
    // Printed whatever the winning rung was. A fit that lost to a number
    // someone typed is still the fact that says whether the number was a good
    // one, and a fit that refused is the fact that explains the floor.
    println!("  {:<22} {}", "fitted to hardware", opt(fitted));
    println!("  {:<22} {}", "  device budget", gib(inputs.budget_bytes));
    println!(
        "  {:<22} {}",
        resident_label(model),
        gib(inputs.weights_bytes)
    );
    println!(
        "  {:<22} {}",
        "  kv bytes/token",
        opt(inputs.kv_bytes_per_token)
    );
    println!("  {:<22} {}", "  trained window", opt(inputs.trained_ctx));
    // The gap between these two is what the ladder costs, which is the whole
    // of ADR 0009's first kill criterion.
    println!("  {:<22} {}", "  before snapping", opt(inputs.unsnapped));
}

/// The context this machine would fit for `model`, and what the fit worked
/// from.
///
/// Sized by [`resident_bytes`], the figure a launch of the model is sized by,
/// so a model with a projector is fitted here as it is when it starts.
fn context_fit(model: &Model) -> (Option<u64>, FitInputs) {
    let kv = gglib_core::domain::estimate_kv_elems_per_token(
        &model.metadata,
        model.architecture.as_deref(),
    );
    let kv_types = resolve_kv_cache_types(None, None);
    explain_fit(
        model.context_length,
        Some(resident_bytes(model)),
        kv,
        kv_types.k,
        kv_types.v,
    )
}

/// What the fit's resident figure is made of: the weights, and the projector
/// a launch loads beside them when the model is linked to one.
const fn resident_label(model: &Model) -> &'static str {
    if model.image_input() {
        "  weights + projector"
    } else {
        "  weights"
    }
}

/// `None` reads as a refusal here, not as a zero — see `FitInputs`.
fn opt(v: Option<u64>) -> String {
    v.map_or_else(|| "unknown".to_owned(), |n| n.to_string())
}

/// Bytes as GiB, or `unknown` when the value could not be read.
fn gib(v: Option<u64>) -> String {
    v.map_or_else(
        || "unknown".to_owned(),
        |b| {
            #[allow(clippy::cast_precision_loss)]
            let g = b as f64 / (1024.0 * 1024.0 * 1024.0);
            format!("{g:.2} GiB")
        },
    )
}

#[cfg(test)]
#[path = "explain_tests.rs"]
mod tests;
