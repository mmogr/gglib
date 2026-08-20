//! Advertising `{model}:{profile}` variants in `/v1/models`.
//!
//! Selecting a profile is [`gglib_core::request_pipeline::resolve_route`];
//! this module is the other half — deciding which `{model}:{profile}` ids the
//! proxy *offers* a client in its model list, so they are directly selectable
//! in a picker rather than only typeable by hand.
//!
//! Listing is opt-in per profile because the full cross product of models and
//! profiles would swamp the very picker it exists to serve. An unlisted
//! profile stays fully usable by name.

use gglib_core::domain::InferenceProfile;

use crate::models::ModelInfo;

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
pub(crate) fn variant_entries(
    models: &[ModelInfo],
    profiles: &[InferenceProfile],
) -> Vec<ModelInfo> {
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
                // Inherited like `context_window`: a profile only changes
                // sampling parameters, so it cannot add or remove an endpoint
                // the base model can serve.
                capabilities: model.capabilities.clone(),
            })
        })
        .collect()
}

/// Comma-separated list of configured profile names, for error messages.
///
/// Returns `None` when no profiles are configured, so the caller can say so
/// explicitly rather than printing an empty list.
#[must_use]
pub(crate) fn configured_names(profiles: &[InferenceProfile]) -> Option<String> {
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
    use gglib_core::ports::ModelCatalogPort;
    use gglib_core::ports::model_catalog::{CatalogError, ModelLaunchSpec, ModelSummary};
    use gglib_core::request_pipeline::{ModelRoute, resolve_route};

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
                dialect: None,
                template_caps: None,
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
                defaults_origin: None,
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
            capabilities: None,
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
