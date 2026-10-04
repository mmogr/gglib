//! `gglib model explain` handler.
//!
//! Resolves a model's sampling parameters through the same hierarchy the
//! proxy uses and prints each one beside the layer that supplied it.
//!
//! The resolution is [`gglib_core::request_pipeline::explain_stored`] —
//! the identical call the plain resolution makes, returning the provenance it
//! otherwise discards. Nothing here re-implements the ladder, so this command
//! cannot describe a hierarchy that differs from the one that runs.

use anyhow::{Result, anyhow};
use gglib_core::Settings;
use gglib_core::domain::{
    FitInputs, InferenceProfile, Model, ModelSamplingContext, ModelSamplingDefaults,
};
use gglib_core::request_pipeline;
use gglib_core::server_config::{ServerConfigOptions, resolve_context_size_with_source};
use gglib_runtime::llama::args::resolve_kv_cache_types;
use gglib_runtime::ports_impl::model_shards::resident_bytes;
use gglib_runtime::process::residency::explain::explain_fit;

use super::resolver;
use crate::bootstrap::CliContext;
use crate::handlers::config::settings::profiles::not_found_message;
use crate::presentation::explain_display::{self, ExplainContext};

/// Execute `gglib model explain <id> [--profile NAME]`.
pub(crate) async fn execute(
    ctx: &CliContext,
    identifier: &str,
    profile: Option<&str>,
) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    let settings = ctx.app.settings().get().await?;

    let selected = match profile {
        Some(name) => Some(find_profile(name, settings.inference_profiles.as_deref())?),
        None => None,
    };

    // The two facts about the model that change how resolution behaves. Built
    // through the same constructor the live path uses, so this command cannot
    // explain a resolution that differs from the one that runs — which is the
    // entire value of the command.
    let model_ctx = ModelSamplingContext::for_model(&model);

    // An empty request layer: this command explains the stored configuration,
    // so there are no per-request parameters to occupy the top rung.
    let (resolved, sources, effort_suppressed) = request_pipeline::explain_stored(
        selected.as_ref(),
        model.inference_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
        model_ctx,
        &model.template_caps,
    );

    // The request pipeline's stage 5b, applied to the resolution rather than to
    // a request — the shared predicate, for the reason this whole command
    // exists: an explanation that re-derived the condition could describe a
    // hierarchy, or a gate, that differs from the one that runs.
    //
    // A no-op unless this model's recorded template caps positively say the
    // template does not read `reasoning_effort`. On the common model — never
    // launched, so never probed — the answer is `Unknown` and the level stands.

    explain_display::print_explanation(
        &model.name,
        model.id,
        &resolved,
        &sources,
        ExplainContext {
            profile: selected.as_ref().map(|p| p.name.as_str()),
            is_reasoning: model_ctx.is_reasoning,
            trust_client_sampling: settings.trust_client_sampling.unwrap_or(false),
            // Read from the same stored GGUF metadata the baseline check reads,
            // so `explain` and the proxy's readback cannot disagree about what
            // this model published.
            model_sampling: ModelSamplingDefaults::from_metadata(&model.metadata),
            defaults_origin: model.defaults_origin,
            effort_suppressed,
        },
    );

    print_context_explanation(&model, &settings);

    Ok(())
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

/// Look up a configured profile by name.
///
/// Errors rather than falling back to no profile: someone who passed
/// `--profile` wants to see that profile's effect, and silently showing them
/// the unprofiled resolution would answer a question they did not ask.
fn find_profile(name: &str, profiles: Option<&[InferenceProfile]>) -> Result<InferenceProfile> {
    let profiles = profiles.unwrap_or_default();
    profiles
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .ok_or_else(|| anyhow!(not_found_message(name, profiles)))
}

#[cfg(test)]
mod tests {
    use super::*;

    use gglib_core::domain::InferenceConfig;

    fn profile(name: &str) -> InferenceProfile {
        InferenceProfile {
            name: name.to_owned(),
            description: None,
            config: InferenceConfig::default(),
            list_in_models: false,
        }
    }

    #[test]
    fn finds_a_configured_profile_by_name() {
        let profiles = vec![profile("coding"), profile("chat")];
        assert_eq!(
            find_profile("chat", Some(&profiles)).unwrap().name,
            "chat".to_owned()
        );
    }

    /// The error names what does exist, so a typo is self-correcting.
    #[test]
    fn an_unknown_profile_errors_and_lists_the_configured_ones() {
        let profiles = vec![profile("coding")];
        let err = find_profile("codign", Some(&profiles))
            .unwrap_err()
            .to_string();

        assert!(err.contains("codign"), "{err}");
        assert!(err.contains("coding"), "{err}");
    }

    fn model(weights: &std::path::Path, projector: Option<&std::path::Path>) -> Model {
        let mut new = gglib_core::NewModel::new(
            "qwen".to_owned(),
            weights.to_path_buf(),
            7.0,
            chrono::Utc::now(),
        );
        new.projector_path = projector.map(std::path::Path::to_path_buf);
        Model::stored(1, &new)
    }

    /// The resident figure is labelled for what `resident_bytes` summed.
    #[test]
    fn the_resident_figure_names_the_projector_when_it_counts_one() {
        let weights = std::path::Path::new("/models/qwen.gguf");
        let projector = std::path::Path::new("/models/mmproj-F16.gguf");

        assert_eq!(resident_label(&model(weights, None)), "  weights");
        assert_eq!(
            resident_label(&model(weights, Some(projector))),
            "  weights + projector"
        );
    }

    /// The figure under that label: both files, and the same number a launch
    /// of the model is sized by.
    #[test]
    fn the_fit_is_sized_by_the_weights_and_the_projector_as_a_launch_is() {
        let dir = tempfile::tempdir().unwrap();
        let weights = dir.path().join("qwen.Q8_0.gguf");
        let projector = dir.path().join("mmproj-F16.gguf");
        std::fs::write(&weights, vec![0u8; 4096]).unwrap();
        std::fs::write(&projector, vec![0u8; 512]).unwrap();
        let linked = model(&weights, Some(&projector));

        let (_, inputs) = context_fit(&linked);

        assert_eq!(inputs.weights_bytes, Some(4608));
        let launch = gglib_runtime::ports_impl::model_catalog::model_to_launch_spec(linked);
        assert_eq!(inputs.weights_bytes, Some(launch.file_size_bytes));
    }

    #[test]
    fn the_fit_of_an_unlinked_model_is_sized_by_its_weights() {
        let dir = tempfile::tempdir().unwrap();
        let weights = dir.path().join("qwen.Q8_0.gguf");
        std::fs::write(&weights, vec![0u8; 4096]).unwrap();

        let (_, inputs) = context_fit(&model(&weights, None));

        assert_eq!(inputs.weights_bytes, Some(4096));
    }

    /// With no profiles configured at all the message should point at the
    /// command that creates some, rather than listing an empty set.
    #[test]
    fn an_unset_profile_list_is_not_an_empty_list() {
        let err = find_profile("coding", None).unwrap_err().to_string();
        assert!(err.contains("install-templates"), "{err}");
    }
}
