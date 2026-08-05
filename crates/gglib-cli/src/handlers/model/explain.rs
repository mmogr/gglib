//! `gglib model explain` handler.
//!
//! Resolves a model's sampling parameters through the same hierarchy the
//! proxy uses and prints each one beside the layer that supplied it.
//!
//! The resolution is [`InferenceConfig::resolve_with_profile_explained`] —
//! the identical call the plain resolution makes, returning the provenance it
//! otherwise discards. Nothing here re-implements the ladder, so this command
//! cannot describe a hierarchy that differs from the one that runs.

use anyhow::{Result, anyhow};
use gglib_core::domain::{InferenceConfig, InferenceProfile, ModelSamplingContext};

use super::resolver;
use crate::bootstrap::CliContext;
use crate::handlers::config::settings::profiles::not_found_message;
use crate::presentation::explain_display::{self, ExplainContext};

/// Execute `gglib model explain <id> [--profile NAME]`.
pub async fn execute(ctx: &CliContext, identifier: &str, profile: Option<&str>) -> Result<()> {
    let model = resolver::resolve_model_identifier(ctx, identifier).await?;
    let settings = ctx.app.settings().get().await?;

    let selected = match profile {
        Some(name) => Some(find_profile(name, settings.inference_profiles.as_deref())?),
        None => None,
    };

    // The two facts about the model that change how resolution behaves; built
    // exactly as `handlers::inference::shared` builds them for the live path.
    let model_ctx = ModelSamplingContext {
        is_reasoning: is_reasoning(&model.tags),
        defaults_origin: model.defaults_origin,
    };

    // An empty request layer: this command explains the stored configuration,
    // so there are no per-request parameters to occupy the top rung.
    let (resolved, sources) = InferenceConfig::default().resolve_with_profile_explained(
        selected.as_ref().map(|p| &p.config),
        model.inference_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
        model_ctx,
    );

    explain_display::print_explanation(
        &model.name,
        model.id,
        &resolved,
        &sources,
        ExplainContext {
            profile: selected.as_ref().map(|p| p.name.as_str()),
            is_reasoning: model_ctx.is_reasoning,
            trust_client_sampling: settings.trust_client_sampling.unwrap_or(false),
        },
    );

    Ok(())
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

/// Whether the model carries the `reasoning` tag, which selects the floor.
fn is_reasoning(tags: &[String]) -> bool {
    tags.iter().any(|tag| tag.eq_ignore_ascii_case("reasoning"))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// With no profiles configured at all the message should point at the
    /// command that creates some, rather than listing an empty set.
    #[test]
    fn an_unset_profile_list_is_not_an_empty_list() {
        let err = find_profile("coding", None).unwrap_err().to_string();
        assert!(err.contains("install-templates"), "{err}");
    }

    #[test]
    fn the_reasoning_tag_is_matched_case_insensitively() {
        assert!(is_reasoning(&["Reasoning".to_owned()]));
        assert!(is_reasoning(&["moe".to_owned(), "reasoning".to_owned()]));
        assert!(!is_reasoning(&["code".to_owned()]));
        assert!(!is_reasoning(&[]));
    }
}
