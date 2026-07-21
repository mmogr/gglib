//! Routing of `{model}:{profile}` request ids.
//!
//! A client selects a named sampling profile by suffixing the model it asks
//! for — `qwen3.6:coding`. This module decides, for one requested id, whether
//! that id names a model outright, a model plus a configured profile, or a
//! profile that does not exist.
//!
//! # Why the catalog decides, not a pattern
//!
//! Colons are legitimate inside model names: Ollama-style `name:tag` ids are
//! everywhere, so `qwen3.6:27b` may well *be* a model. A purely lexical rule
//! cannot tell that apart from `qwen3.6:coding` naming a profile, so this
//! module asks the catalog instead. A full-string catalog hit always wins,
//! which means adding profiles can never shadow a model that already exists.
//!
//! # Why an unmatched suffix is an error
//!
//! When the suffix matches no profile and the *base* is a real model, the
//! request is not forwarded — it fails with 404. Silently falling back to the
//! bare model is the dangerous option: a coding agent whose profile was
//! renamed or deleted would keep working while quietly sampling at the wrong
//! temperature, which is exactly the failure this feature exists to prevent. A
//! loud 404 at the moment of the rename is far cheaper to diagnose.
//!
//! That branch cannot distinguish a deleted profile from a model tag that was
//! never in the catalog (`qwen3.6:27b` with no such model *and* no `27b`
//! profile), so the error names both readings rather than guessing.
//!
//! # Cost
//!
//! An id with no `:` returns immediately with no catalog access at all, which
//! is every request from a client that does not use profiles. Only
//! colon-bearing ids reach the catalog.

use gglib_core::domain::InferenceProfile;
use gglib_core::ports::ModelCatalogPort;
use tracing::{debug, warn};

use crate::models::ModelInfo;

/// What a requested model id turned out to mean.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelRoute<'a> {
    /// The id names a model directly; no profile applies.
    ///
    /// Also the outcome when neither the full id nor its base resolves — the
    /// request continues to the normal model-not-found path rather than being
    /// second-guessed here.
    Bare(&'a str),

    /// The id named a model plus a configured profile.
    Profiled {
        /// The base model name, with the profile suffix removed.
        model: &'a str,
        /// The selected profile.
        profile: &'a InferenceProfile,
    },

    /// The base names a real model but the suffix matches no configured
    /// profile.
    ProfileNotFound {
        /// The full id as requested, for the error message.
        requested: &'a str,
        /// The suffix that failed to match.
        suffix: &'a str,
    },
}

/// Resolve a requested model id into a [`ModelRoute`].
///
/// Resolution order, first match wins:
///
/// 1. No `:` in the id — [`ModelRoute::Bare`], without touching the catalog.
/// 2. The full id resolves in the catalog — [`ModelRoute::Bare`]. A real model
///    whose name contains a colon always beats a profile reading.
/// 3. The suffix after the last `:` matches a configured profile —
///    [`ModelRoute::Profiled`].
/// 4. The base resolves in the catalog — [`ModelRoute::ProfileNotFound`].
/// 5. Otherwise [`ModelRoute::Bare`], leaving the existing model-not-found
///    path to report it.
///
/// Splitting on the *last* colon lets a colon-bearing model name still carry a
/// profile (`qwen:27b:coding`).
///
/// Catalog errors are treated as "not found" and logged: a degraded catalog
/// should not turn into a hard failure on a request that may not need a profile
/// at all.
pub async fn resolve_route<'a>(
    requested: &'a str,
    profiles: &'a [InferenceProfile],
    catalog: &dyn ModelCatalogPort,
) -> ModelRoute<'a> {
    // 1. The overwhelmingly common case: no profile suffix, no catalog access.
    let Some((base, suffix)) = requested.rsplit_once(':') else {
        return ModelRoute::Bare(requested);
    };

    // 2. A model that genuinely owns this name wins outright.
    if model_exists(catalog, requested).await {
        return ModelRoute::Bare(requested);
    }

    // 3. A configured profile.
    if let Some(profile) = profiles.iter().find(|p| p.name == suffix) {
        debug!(model = %base, profile = %suffix, "resolved model:profile request");
        return ModelRoute::Profiled {
            model: base,
            profile,
        };
    }

    // 4. Base is real, suffix means nothing — fail loudly rather than sample
    //    at the wrong temperature without saying so.
    if model_exists(catalog, base).await {
        warn!(
            requested = %requested,
            suffix = %suffix,
            "request names no configured profile; rejecting rather than falling back"
        );
        return ModelRoute::ProfileNotFound { requested, suffix };
    }

    // 5. Nothing matched. Let the normal model-not-found path speak.
    ModelRoute::Bare(requested)
}

/// Whether `name` resolves in the catalog, treating a query failure as absent.
async fn model_exists(catalog: &dyn ModelCatalogPort, name: &str) -> bool {
    match catalog.resolve_model(name).await {
        Ok(found) => found.is_some(),
        Err(e) => {
            warn!(model = %name, error = %e, "catalog lookup failed during profile routing");
            false
        }
    }
}

/// Build the `{model}:{profile}` entries to advertise in `/v1/models`.
///
/// Only profiles with `list_in_models` set are advertised. The full cross
/// product of models and profiles would swamp a client's model picker — with a
/// handful of each it runs to dozens of entries — so listing is opt-in per
/// profile, and unlisted profiles stay perfectly usable by name.
///
/// Variants inherit the base model's `context_window`: a profile changes
/// sampling only, never how much context the model was launched with, so
/// advertising anything else would mislead clients that budget against it.
///
/// `models` must be the base catalog entries only. Passing entries that already
/// include variants would compound them into `{model}:{a}:{b}`.
#[must_use]
pub fn variant_entries(models: &[ModelInfo], profiles: &[InferenceProfile]) -> Vec<ModelInfo> {
    let listed: Vec<&InferenceProfile> = profiles.iter().filter(|p| p.list_in_models).collect();
    if listed.is_empty() {
        return Vec::new();
    }

    models
        .iter()
        .flat_map(|model| {
            listed.iter().map(move |profile| ModelInfo {
                id: format!("{}:{}", model.id, profile.name),
                object: "model".to_owned(),
                created: model.created,
                owned_by: "gglib".to_owned(),
                description: Some(profile.description.clone().unwrap_or_else(|| {
                    format!("{} with the '{}' sampling profile", model.id, profile.name)
                })),
                context_window: model.context_window,
            })
        })
        .collect()
}

/// Comma-separated list of configured profile names, for error messages.
///
/// Returns `None` when no profiles are configured, so the caller can say so
/// explicitly rather than printing an empty list.
#[must_use]
pub fn configured_names(profiles: &[InferenceProfile]) -> Option<String> {
    if profiles.is_empty() {
        return None;
    }
    Some(
        profiles
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    use async_trait::async_trait;
    use gglib_core::domain::InferenceConfig;
    use gglib_core::ports::model_catalog::{CatalogError, ModelLaunchSpec, ModelSummary};

    /// Catalog holding a fixed set of names, resolving by exact match — the
    /// behaviour of the real SQLite repository (`WHERE name = ?`).
    #[derive(Debug)]
    struct NamedCatalog {
        names: HashSet<String>,
    }

    impl NamedCatalog {
        fn new(names: &[&str]) -> Self {
            Self {
                names: names.iter().map(|n| (*n).to_owned()).collect(),
            }
        }
    }

    #[async_trait]
    impl ModelCatalogPort for NamedCatalog {
        async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
            Ok(Vec::new())
        }

        async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
            Ok(self.names.contains(name).then(|| ModelSummary {
                id: 1,
                name: name.to_owned(),
                tags: Vec::new(),
                capabilities: gglib_core::domain::ModelCapabilities::empty(),
                param_count: "7B".to_owned(),
                quantization: None,
                architecture: None,
                created_at: 0,
                file_size: 0,
                context_length: None,
                inference_defaults: None,
                server_defaults: None,
            }))
        }

        async fn resolve_for_launch(
            &self,
            _name: &str,
        ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
            Ok(None)
        }
    }

    /// Catalog whose every query fails, to pin the fail-open behaviour.
    #[derive(Debug)]
    struct BrokenCatalog;

    #[async_trait]
    impl ModelCatalogPort for BrokenCatalog {
        async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
            Err(CatalogError::QueryFailed("boom".to_owned()))
        }

        async fn resolve_model(&self, _name: &str) -> Result<Option<ModelSummary>, CatalogError> {
            Err(CatalogError::QueryFailed("boom".to_owned()))
        }

        async fn resolve_for_launch(
            &self,
            _name: &str,
        ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
            Err(CatalogError::QueryFailed("boom".to_owned()))
        }
    }

    fn profiles() -> Vec<InferenceProfile> {
        vec![InferenceProfile {
            name: "coding".to_owned(),
            description: None,
            config: InferenceConfig {
                temperature: Some(0.2),
                ..Default::default()
            },
            list_in_models: false,
        }]
    }

    #[tokio::test]
    async fn plain_model_name_is_bare() {
        let catalog = NamedCatalog::new(&["qwen"]);
        assert_eq!(
            resolve_route("qwen", &profiles(), &catalog).await,
            ModelRoute::Bare("qwen")
        );
    }

    #[tokio::test]
    async fn known_profile_suffix_resolves_to_profiled() {
        let catalog = NamedCatalog::new(&["qwen"]);
        let profiles = profiles();
        match resolve_route("qwen:coding", &profiles, &catalog).await {
            ModelRoute::Profiled { model, profile } => {
                assert_eq!(model, "qwen");
                assert_eq!(profile.name, "coding");
            }
            other => panic!("expected Profiled, got {other:?}"),
        }
    }

    /// A model that genuinely owns a colon-bearing name must win over any
    /// profile reading of its suffix — otherwise adding a profile could
    /// shadow an existing model.
    #[tokio::test]
    async fn real_colon_bearing_model_name_wins_over_profile_suffix() {
        let catalog = NamedCatalog::new(&["qwen:coding", "qwen"]);
        assert_eq!(
            resolve_route("qwen:coding", &profiles(), &catalog).await,
            ModelRoute::Bare("qwen:coding")
        );
    }

    /// The reason this module exists: a renamed or deleted profile must fail
    /// loudly rather than quietly sampling at the model's own temperature.
    #[tokio::test]
    async fn unknown_suffix_on_a_real_model_is_profile_not_found() {
        let catalog = NamedCatalog::new(&["qwen"]);
        assert_eq!(
            resolve_route("qwen:deleted", &profiles(), &catalog).await,
            ModelRoute::ProfileNotFound {
                requested: "qwen:deleted",
                suffix: "deleted",
            }
        );
    }

    /// Nothing resolves — stay out of the way and let the existing
    /// model-not-found path report it.
    #[tokio::test]
    async fn unknown_base_falls_through_to_bare() {
        let catalog = NamedCatalog::new(&["qwen"]);
        assert_eq!(
            resolve_route("mistral:27b", &profiles(), &catalog).await,
            ModelRoute::Bare("mistral:27b")
        );
    }

    /// The council virtual models are matched whole upstream of this module,
    /// but their suffixes are also reserved profile names, so confirm routing
    /// never rewrites one into a base plus profile.
    #[tokio::test]
    async fn council_virtual_model_is_left_intact() {
        let catalog = NamedCatalog::new(&[]);
        assert_eq!(
            resolve_route("gglib-council:interactive", &profiles(), &catalog).await,
            ModelRoute::Bare("gglib-council:interactive")
        );
    }

    /// Splitting on the last colon lets a colon-bearing model still take a
    /// profile.
    #[tokio::test]
    async fn splits_on_the_last_colon() {
        let catalog = NamedCatalog::new(&["qwen:27b"]);
        match resolve_route("qwen:27b:coding", &profiles(), &catalog).await {
            ModelRoute::Profiled { model, profile } => {
                assert_eq!(model, "qwen:27b");
                assert_eq!(profile.name, "coding");
            }
            other => panic!("expected Profiled, got {other:?}"),
        }
    }

    /// With no profiles configured at all, a colon-bearing id must not become
    /// a hard error when it is simply an unknown model.
    #[tokio::test]
    async fn no_profiles_configured_leaves_unknown_ids_bare() {
        let catalog = NamedCatalog::new(&[]);
        assert_eq!(
            resolve_route("qwen:27b", &[], &catalog).await,
            ModelRoute::Bare("qwen:27b")
        );
    }

    /// A broken catalog must not escalate into a request failure.
    #[tokio::test]
    async fn catalog_errors_fail_open_to_bare() {
        assert_eq!(
            resolve_route("qwen:unknown", &profiles(), &BrokenCatalog).await,
            ModelRoute::Bare("qwen:unknown")
        );
    }

    /// A catalog error must not suppress a profile that is configured.
    #[tokio::test]
    async fn catalog_errors_still_allow_a_known_profile() {
        let profiles = profiles();
        match resolve_route("qwen:coding", &profiles, &BrokenCatalog).await {
            ModelRoute::Profiled { model, .. } => assert_eq!(model, "qwen"),
            other => panic!("expected Profiled, got {other:?}"),
        }
    }

    #[test]
    fn configured_names_lists_profiles_or_none() {
        assert_eq!(configured_names(&profiles()), Some("coding".to_owned()));
        assert_eq!(configured_names(&[]), None);
    }

    // ── /v1/models variants ─────────────────────────────────────────────

    fn model_info(id: &str, context_window: Option<u64>) -> ModelInfo {
        ModelInfo {
            id: id.to_owned(),
            object: "model".to_owned(),
            created: 42,
            owned_by: "gglib".to_owned(),
            description: None,
            context_window,
        }
    }

    fn listed(name: &str) -> InferenceProfile {
        InferenceProfile {
            name: name.to_owned(),
            description: Some(format!("the {name} profile")),
            config: InferenceConfig::default(),
            list_in_models: true,
        }
    }

    /// Listing is opt-in, so a profile that is merely configured contributes
    /// nothing to the picker.
    #[test]
    fn only_opted_in_profiles_are_advertised() {
        let models = vec![model_info("qwen", None)];
        // `profiles()` has list_in_models: false.
        assert!(variant_entries(&models, &profiles()).is_empty());

        let entries = variant_entries(&models, &[listed("chat")]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "qwen:chat");
    }

    /// Every model gets every listed profile — the dimension is global, not
    /// per-model.
    #[test]
    fn each_model_gets_each_listed_profile() {
        let models = vec![model_info("qwen", None), model_info("mistral", None)];
        let entries = variant_entries(&models, &[listed("chat"), listed("creative")]);

        let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "qwen:chat",
                "qwen:creative",
                "mistral:chat",
                "mistral:creative"
            ]
        );
    }

    /// A profile changes sampling, not the context the model was launched
    /// with, so advertising anything but the base model's window would
    /// mislead clients that budget against it.
    #[test]
    fn variants_inherit_the_base_context_window() {
        let models = vec![model_info("qwen", Some(8192))];
        let entries = variant_entries(&models, &[listed("chat")]);
        assert_eq!(entries[0].context_window, Some(8192));
    }

    /// A profile without its own description still gets something useful
    /// rather than an empty field.
    #[test]
    fn variants_without_a_description_get_a_generated_one() {
        let mut profile = listed("chat");
        profile.description = None;
        let entries = variant_entries(&[model_info("qwen", None)], &[profile]);

        let description = entries[0].description.as_deref().unwrap_or_default();
        assert!(description.contains("qwen"), "got: {description}");
        assert!(description.contains("chat"), "got: {description}");
    }

    /// Every advertised id must round-trip back through routing to the model
    /// and profile it was built from — otherwise the picker would offer
    /// entries that fail when selected.
    #[tokio::test]
    async fn advertised_ids_resolve_back_to_their_model_and_profile() {
        let catalog = NamedCatalog::new(&["qwen", "qwen:27b"]);
        let configured = vec![listed("chat")];
        let models = vec![model_info("qwen", None), model_info("qwen:27b", None)];

        for entry in variant_entries(&models, &configured) {
            match resolve_route(&entry.id, &configured, &catalog).await {
                ModelRoute::Profiled { model, profile } => {
                    assert!(entry.id.starts_with(model), "{} vs {model}", entry.id);
                    assert_eq!(profile.name, "chat");
                }
                other => panic!("advertised id {} did not resolve: {other:?}", entry.id),
            }
        }
    }
}
