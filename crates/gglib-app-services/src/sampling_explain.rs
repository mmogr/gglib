//! Resolved sampling parameters plus the layer that supplied each one.
//!
//! The wire form of [`FieldSources`](gglib_core::domain::FieldSources), which the surfaces cannot serialize
//! directly: [`ParamSource::Layer`] carries a ladder index rather than a name,
//! and the core provenance types predate the camelCase convention the rest of
//! the HTTP API follows. Translating once here keeps a single description of
//! the shape instead of one per surface.
//!
//! Nothing in this module re-derives the ladder. The values and their
//! provenance both come from
//! [`InferenceConfig::resolve_with_profile_explained`] — the same call the
//! plain resolution makes — so an explanation cannot describe a hierarchy that
//! differs from the one that runs.

use gglib_core::domain::{
    InferenceConfig, InferenceProfile, Model, ModelSamplingContext, ParamSource, SamplingLayer,
};
use gglib_core::settings::Settings;
use serde::{Deserialize, Serialize};

use crate::error::GuiError;

/// A model's resolved sampling parameters and where each value came from.
///
/// Consumers:
/// - Axum: `GET /api/models/:id/explain`
/// - GUI frontend: the inspector's Sampling section
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingExplanationDto {
    /// The fully resolved configuration, after floors.
    ///
    /// Carried whole rather than copied field-by-field into
    /// [`ParamProvenanceDto`], so each value keeps the width it was declared
    /// at and `serde_json` emits the shortest representation that round-trips
    /// — `0.95`, not the `0.949999988079071` a detour through `f64` produces.
    pub resolved: InferenceConfig,
    /// Per-parameter provenance, in [`FieldSources::iter`](gglib_core::domain::FieldSources::iter)'s display order.
    pub sources: Vec<ParamProvenanceDto>,
    /// The profile applied, if the caller named one.
    pub profile: Option<String>,
    /// Whether the model carries the `reasoning` tag, which selects the floor.
    pub is_reasoning: bool,
    /// Whether client-supplied sampling is trusted, which the table cannot
    /// show: that rung is never stored on the model.
    pub trust_client_sampling: bool,
}

/// Where one resolved parameter's value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamProvenanceDto {
    /// The camelCase key this entry describes in
    /// [`SamplingExplanationDto::resolved`], e.g. `topP`.
    pub param: String,
    /// Which class of source supplied the value.
    pub kind: ProvenanceKindDto,
    /// The ladder rung, present only when `kind` is
    /// [`Layer`](ProvenanceKindDto::Layer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer: Option<SamplingLayerDto>,
}

/// The wire form of [`ParamSource`], with the ladder index resolved to a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProvenanceKindDto {
    /// A ladder rung named the value; see [`ParamProvenanceDto::layer`].
    Layer,
    /// The class floor, because no rung named a value at all.
    Floor,
    /// The class floor, because a rung claimed `temperature` and this
    /// parameter is tuned against it.
    FloorCoupled,
    /// Nothing named it and the floor carries none either.
    Unset,
}

/// The wire form of [`SamplingLayer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SamplingLayerDto {
    /// Caller-supplied overrides — request parameters or CLI flags.
    Request,
    /// The named profile the caller selected.
    Profile,
    /// Per-model defaults a person tuned deliberately.
    ModelUserSet,
    /// Global settings defaults.
    Global,
    /// Per-model defaults written automatically from the `reasoning` tag.
    ModelAutoDetected,
}

impl From<SamplingLayer> for SamplingLayerDto {
    fn from(layer: SamplingLayer) -> Self {
        match layer {
            SamplingLayer::Request => Self::Request,
            SamplingLayer::Profile => Self::Profile,
            SamplingLayer::ModelUserSet => Self::ModelUserSet,
            SamplingLayer::Global => Self::Global,
            SamplingLayer::ModelAutoDetected => Self::ModelAutoDetected,
        }
    }
}

/// The camelCase key one [`FieldSources`](gglib_core::domain::FieldSources) field occupies in a serialized
/// [`InferenceConfig`].
///
/// A field this does not know is returned unchanged, which produces a `param`
/// that indexes nothing rather than a panic in a read-only view. The pairing
/// is pinned by `every_param_is_a_key_of_the_resolved_config` below, so a new
/// field on `FieldSources` fails a test rather than reaching a client.
fn wire_key(field: &'static str) -> &'static str {
    match field {
        "top_p" => "topP",
        "top_k" => "topK",
        "presence_penalty" => "presencePenalty",
        "repeat_penalty" => "repeatPenalty",
        "min_p" => "minP",
        "dry_multiplier" => "dryMultiplier",
        "dry_base" => "dryBase",
        "dry_allowed_length" => "dryAllowedLength",
        "dry_penalty_last_n" => "dryPenaltyLastN",
        "max_tokens" => "maxTokens",
        other => other,
    }
}

/// Translate one `(field, source)` pair into its wire form.
fn provenance(field: &'static str, source: ParamSource) -> ParamProvenanceDto {
    let (kind, layer) = match source {
        // `from_index` returns `None` only for a ladder longer than the five
        // rungs `resolve_with_profile_explained` builds, which this module
        // never resolves. Surface it as a nameless layer rather than guessing.
        ParamSource::Layer(index) => (
            ProvenanceKindDto::Layer,
            SamplingLayer::from_index(index).map(SamplingLayerDto::from),
        ),
        ParamSource::Floor => (ProvenanceKindDto::Floor, None),
        ParamSource::FloorCoupled => (ProvenanceKindDto::FloorCoupled, None),
        ParamSource::Unset => (ProvenanceKindDto::Unset, None),
    };

    ParamProvenanceDto {
        param: wire_key(field).to_owned(),
        kind,
        layer,
    }
}

/// Look up a configured profile by name.
///
/// Errors rather than falling back to no profile: someone who named a profile
/// wants to see that profile's effect, and silently showing them the
/// unprofiled resolution would answer a question they did not ask. The message
/// names what does exist, so a typo is self-correcting.
pub(crate) fn find_profile<'a>(
    name: &str,
    profiles: Option<&'a [InferenceProfile]>,
) -> Result<&'a InferenceProfile, GuiError> {
    let profiles = profiles.unwrap_or_default();
    profiles
        .iter()
        .find(|profile| profile.name == name)
        .ok_or_else(|| {
            let names = profiles
                .iter()
                .map(|profile| profile.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            GuiError::ValidationFailed(if names.is_empty() {
                format!("no profile named '{name}'; none are configured")
            } else {
                format!("no profile named '{name}'; configured profiles are: {names}")
            })
        })
}

/// Resolve a model's sampling parameters and describe where each came from.
pub(crate) fn explain(
    model: &Model,
    settings: &Settings,
    profile: Option<&InferenceProfile>,
) -> SamplingExplanationDto {
    // The two facts about the model that change how resolution behaves.
    let model_ctx = ModelSamplingContext {
        is_reasoning: is_reasoning(&model.tags),
        defaults_origin: model.defaults_origin,
    };

    // An empty request layer: this explains the stored configuration, so there
    // are no per-request parameters to occupy the top rung.
    let (resolved, sources) = InferenceConfig::default().resolve_with_profile_explained(
        profile.map(|selected| &selected.config),
        model.inference_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
        model_ctx,
    );

    SamplingExplanationDto {
        resolved,
        sources: sources
            .iter()
            .map(|(field, source)| provenance(field, source))
            .collect(),
        profile: profile.map(|selected| selected.name.clone()),
        is_reasoning: model_ctx.is_reasoning,
        trust_client_sampling: settings.trust_client_sampling.unwrap_or(false),
    }
}

/// Whether the model carries the `reasoning` tag, which selects the floor.
fn is_reasoning(tags: &[String]) -> bool {
    tags.iter().any(|tag| tag.eq_ignore_ascii_case("reasoning"))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use chrono::Utc;
    use gglib_core::ModelCapabilities;
    use gglib_core::domain::DefaultsOrigin;
    use serde_json::json;

    use super::*;

    /// A model with nothing sampling-relevant set. `Model` has no `Default`,
    /// so the variants below build on this with struct-update syntax.
    fn model() -> Model {
        Model {
            dialect_spec: None,
            id: 1,
            name: "test-model".to_owned(),
            model_key: String::new(),
            file_path: PathBuf::from("/models/test.gguf"),
            param_count_b: 7.0,
            architecture: None,
            quantization: None,
            context_length: None,
            expert_count: None,
            expert_used_count: None,
            expert_shared_count: None,
            metadata: HashMap::new(),
            added_at: Utc::now(),
            hf_repo_id: None,
            hf_commit_sha: None,
            hf_filename: None,
            download_date: None,
            last_update_check: None,
            tags: Vec::new(),
            capabilities: ModelCapabilities::default(),
            inference_defaults: None,
            defaults_origin: None,
            server_defaults: None,
            benchmark_summary: None,
        }
    }

    fn profile(name: &str, config: InferenceConfig) -> InferenceProfile {
        InferenceProfile {
            name: name.to_owned(),
            description: None,
            config,
            list_in_models: false,
        }
    }

    fn source_for<'a>(dto: &'a SamplingExplanationDto, param: &str) -> &'a ParamProvenanceDto {
        dto.sources
            .iter()
            .find(|entry| entry.param == param)
            .unwrap_or_else(|| panic!("no provenance entry for {param}"))
    }

    #[test]
    fn finds_a_configured_profile_by_name() {
        let profiles = vec![profile("coding", InferenceConfig::default())];
        assert_eq!(
            find_profile("coding", Some(&profiles)).unwrap().name,
            "coding"
        );
    }

    #[test]
    fn an_unknown_profile_errors_and_lists_the_configured_ones() {
        let profiles = vec![profile("coding", InferenceConfig::default())];
        let err = find_profile("codign", Some(&profiles))
            .unwrap_err()
            .to_string();

        assert!(err.contains("codign"), "{err}");
        assert!(err.contains("coding"), "{err}");
    }

    /// An empty list is a different situation from a typo, and saying so saves
    /// the reader from hunting for a profile that was never there.
    #[test]
    fn an_unset_profile_list_is_not_an_empty_list() {
        let err = find_profile("coding", None).unwrap_err().to_string();
        assert!(err.contains("none are configured"), "{err}");
    }

    #[test]
    fn a_layer_index_resolves_to_its_rung_name() {
        let entry = provenance("temperature", ParamSource::Layer(2));
        assert_eq!(entry.kind, ProvenanceKindDto::Layer);
        assert_eq!(entry.layer, Some(SamplingLayerDto::ModelUserSet));
    }

    #[test]
    fn the_floor_variants_stay_distinguishable_and_carry_no_layer() {
        let floor = provenance("top_p", ParamSource::Floor);
        assert_eq!(floor.kind, ProvenanceKindDto::Floor);
        assert_eq!(floor.layer, None);

        let coupled = provenance("min_p", ParamSource::FloorCoupled);
        assert_eq!(coupled.kind, ProvenanceKindDto::FloorCoupled);
        assert_eq!(coupled.layer, None);

        let unset = provenance("max_tokens", ParamSource::Unset);
        assert_eq!(unset.kind, ProvenanceKindDto::Unset);
        assert_eq!(unset.layer, None);
    }

    /// A ladder longer than the five rungs this module resolves cannot happen
    /// today; render it as a nameless layer rather than panicking in a
    /// read-only view.
    #[test]
    fn an_index_past_the_ladder_is_a_layer_without_a_name() {
        let entry = provenance("temperature", ParamSource::Layer(9));
        assert_eq!(entry.kind, ProvenanceKindDto::Layer);
        assert_eq!(entry.layer, None);
    }

    /// The client zips `sources[i].param` against `resolved`. If a field on
    /// `FieldSources` ever stops matching a key of the serialized config, that
    /// lookup silently yields nothing — so pin the pairing here.
    #[test]
    fn every_param_is_a_key_of_the_resolved_config() {
        let populated = InferenceConfig {
            temperature: Some(0.7),
            top_p: Some(0.95),
            top_k: Some(40),
            max_tokens: Some(512),
            repeat_penalty: Some(1.0),
            presence_penalty: Some(0.0),
            min_p: Some(0.0),
            dry_multiplier: Some(0.8),
            dry_base: Some(1.75),
            dry_allowed_length: Some(2),
            dry_penalty_last_n: Some(-1),
        };
        let keys = serde_json::to_value(&populated).unwrap();
        let keys = keys.as_object().expect("config serializes as an object");

        let dto = explain(&model(), &Settings::with_defaults(), None);
        assert_eq!(dto.sources.len(), keys.len());
        for entry in &dto.sources {
            assert!(
                keys.contains_key(&entry.param),
                "'{}' is not a key of the resolved config",
                entry.param
            );
        }
    }

    /// The wire contract the frontend is written against.
    ///
    /// Asserted against the serialized *string*, which is what `axum::Json`
    /// writes, rather than `to_value`: the `Value` serializer widens `f32` to
    /// `f64` and would report a `topP` of `0.949999988079071` that no client
    /// ever receives.
    #[test]
    fn serializes_camel_case_with_the_layer_omitted_for_floors() {
        let dto = explain(&model(), &Settings::with_defaults(), None);
        let wire = serde_json::to_string(&dto).unwrap();
        let value: serde_json::Value = serde_json::from_str(&wire).unwrap();

        assert_eq!(value["resolved"]["topP"], json!(0.95), "{wire}");
        assert_eq!(value["isReasoning"], json!(false));
        assert_eq!(value["trustClientSampling"], json!(false));
        assert_eq!(value["profile"], json!(null));
        assert_eq!(
            value["sources"][0],
            json!({ "param": "temperature", "kind": "floor" })
        );
        // maxTokens is last in the canonical order, after the DRY block.
        assert_eq!(
            value["sources"][10],
            json!({ "param": "maxTokens", "kind": "unset" })
        );
    }

    /// Order is the contract: the client renders `sources` as it arrives.
    #[test]
    fn sources_arrive_in_the_canonical_display_order() {
        let dto = explain(&model(), &Settings::with_defaults(), None);
        let params: Vec<&str> = dto.sources.iter().map(|e| e.param.as_str()).collect();
        assert_eq!(
            params,
            [
                "temperature",
                "topP",
                "topK",
                "presencePenalty",
                "repeatPenalty",
                "minP",
                "dryMultiplier",
                "dryBase",
                "dryAllowedLength",
                "dryPenaltyLastN",
                "maxTokens",
            ]
        );
    }

    #[test]
    fn a_model_with_nothing_stored_resolves_entirely_from_the_floor() {
        let dto = explain(&model(), &Settings::with_defaults(), None);

        assert_eq!(dto.resolved.temperature, Some(0.7));
        assert_eq!(
            source_for(&dto, "temperature").kind,
            ProvenanceKindDto::Floor
        );
        assert_eq!(source_for(&dto, "maxTokens").kind, ProvenanceKindDto::Unset);
        assert!(!dto.is_reasoning);
    }

    #[test]
    fn a_profile_outranks_global_settings_and_is_echoed_back() {
        let settings = Settings {
            inference_defaults: Some(InferenceConfig {
                temperature: Some(0.4),
                top_k: Some(15),
                ..Default::default()
            }),
            ..Settings::with_defaults()
        };
        let coding = profile(
            "coding",
            InferenceConfig {
                temperature: Some(0.2),
                ..Default::default()
            },
        );

        let dto = explain(&model(), &settings, Some(&coding));

        assert_eq!(dto.profile.as_deref(), Some("coding"));
        assert_eq!(dto.resolved.temperature, Some(0.2));
        assert_eq!(
            source_for(&dto, "temperature").layer,
            Some(SamplingLayerDto::Profile)
        );
        // Untouched by the profile, so the lower rung still fills it.
        assert_eq!(dto.resolved.top_k, Some(15));
        assert_eq!(
            source_for(&dto, "topK").layer,
            Some(SamplingLayerDto::Global)
        );
    }

    /// The distinction #688 introduced, which is the whole reason a per-field
    /// view beats the stored-defaults view it replaces.
    #[test]
    fn auto_detected_model_defaults_rank_below_global_settings() {
        let settings = Settings {
            inference_defaults: Some(InferenceConfig {
                temperature: Some(0.4),
                ..Default::default()
            }),
            ..Settings::with_defaults()
        };
        let auto = Model {
            dialect_spec: None,
            inference_defaults: Some(InferenceConfig {
                temperature: Some(1.0),
                ..Default::default()
            }),
            defaults_origin: Some(DefaultsOrigin::AutoDetected),
            ..model()
        };

        let dto = explain(&auto, &settings, None);

        assert_eq!(dto.resolved.temperature, Some(0.4));
        assert_eq!(
            source_for(&dto, "temperature").layer,
            Some(SamplingLayerDto::Global)
        );

        let user = Model {
            dialect_spec: None,
            defaults_origin: Some(DefaultsOrigin::User),
            ..auto
        };
        let dto = explain(&user, &settings, None);

        assert_eq!(dto.resolved.temperature, Some(1.0));
        assert_eq!(
            source_for(&dto, "temperature").layer,
            Some(SamplingLayerDto::ModelUserSet)
        );
    }

    /// The tag changes both the flag the client renders and the floor the
    /// coupled set falls back to.
    #[test]
    fn the_reasoning_tag_selects_the_reasoning_floor() {
        let reasoning = Model {
            dialect_spec: None,
            tags: vec!["Reasoning".to_owned()],
            ..model()
        };

        let dto = explain(&reasoning, &Settings::with_defaults(), None);

        assert!(dto.is_reasoning);
        assert_eq!(dto.resolved.presence_penalty, Some(1.0));
    }

    /// A layer that claims `temperature` passes over lower rungs for the trio
    /// tuned against it — the case a bare value cannot explain.
    #[test]
    fn a_claimed_temperature_couples_the_trio_to_the_floor() {
        let coding = profile(
            "coding",
            InferenceConfig {
                temperature: Some(0.2),
                ..Default::default()
            },
        );
        let tuned = Model {
            dialect_spec: None,
            inference_defaults: Some(InferenceConfig {
                presence_penalty: Some(1.5),
                ..Default::default()
            }),
            defaults_origin: Some(DefaultsOrigin::User),
            ..model()
        };

        let dto = explain(&tuned, &Settings::with_defaults(), Some(&coding));

        assert_eq!(dto.resolved.presence_penalty, Some(0.0));
        assert_eq!(
            source_for(&dto, "presencePenalty").kind,
            ProvenanceKindDto::FloorCoupled
        );
    }

    #[test]
    fn trust_client_sampling_is_echoed_from_settings() {
        let settings = Settings {
            trust_client_sampling: Some(true),
            ..Settings::with_defaults()
        };
        assert!(explain(&model(), &settings, None).trust_client_sampling);
    }
}
