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
    DefaultsOrigin, InferenceConfig, InferenceProfile, Model, ModelSamplingContext,
    ModelSamplingDefaults, ParamSource, SamplingLayer, SamplingOverride,
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
    /// What this model's own GGUF publishes, for the fields where it publishes
    /// anything at all.
    ///
    /// Empty on almost every model, and empty is the *interesting* default:
    /// carrying a `notPublished` entry per field would put five noise rows on
    /// every ordinary model to say nothing. A parameter absent from this list
    /// has no author recommendation to honour or displace.
    ///
    /// The provenance column alone cannot answer this. Since llama.cpp PR
    /// #17120 a `general.sampling.*` key is the server's default for every
    /// field gglib does not name, so `unset` means *the model's own number
    /// applies* on a model that published one, and *the build's default
    /// applies* on a model that did not — and those render identically without
    /// this.
    #[serde(default)]
    pub published: Vec<PublishedDefaultDto>,
    /// Where the model's stored defaults came from.
    ///
    /// `Published` and `AutoDetected` share a ladder rung — both are
    /// unreviewed, so both rank below global settings — which means
    /// [`ParamProvenanceDto::layer`] alone cannot name its own source. Without
    /// this, a recipe fetched from the model author renders as gglib's
    /// reasoning-tag guess.
    #[serde(default)]
    pub defaults_origin: Option<DefaultsOriginDto>,
}

/// The wire form of [`DefaultsOrigin`](gglib_core::domain::DefaultsOrigin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DefaultsOriginDto {
    /// Set by a person. Outranks global settings.
    User,
    /// gglib's own `reasoning`-tag guess. Ranks below global settings.
    AutoDetected,
    /// Read from the model author's `generation_config.json` at import.
    /// Ranks below global settings, exactly where `AutoDetected` does.
    Published,
}

impl From<DefaultsOrigin> for DefaultsOriginDto {
    fn from(origin: DefaultsOrigin) -> Self {
        match origin {
            DefaultsOrigin::User => Self::User,
            DefaultsOrigin::AutoDetected => Self::AutoDetected,
            DefaultsOrigin::Published => Self::Published,
        }
    }
}

/// What gglib does with one field's published recommendation.
///
/// The wire form of [`SamplingOverride`](gglib_core::domain::SamplingOverride),
/// minus its `NotPublished` arm — a field with nothing published is omitted
/// from [`SamplingExplanationDto::published`] rather than carried as an empty
/// verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedDefaultDto {
    /// The camelCase key this entry describes, e.g. `topP`. Joins to
    /// [`ParamProvenanceDto::param`] so a surface can render the two together.
    pub param: String,
    /// The GGUF key carrying the value, e.g. `general.sampling.temp`.
    ///
    /// Shown rather than derived, because `repeat_penalty` is published under
    /// `general.sampling.penalty_repeat` and no client should have to know
    /// that.
    pub key: String,
    /// What gglib is doing about it.
    #[serde(flatten)]
    pub state: PublishedStateDto,
}

/// The verdict arm of [`PublishedDefaultDto`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PublishedStateDto {
    /// gglib names nothing, so llama.cpp applies the model author's value.
    Deferred {
        /// What the model author published, and what the sampler will use.
        published: f64,
    },
    /// gglib sends the same number the model published.
    Restated {
        /// The value both sides name.
        published: f64,
    },
    /// gglib sends a different number than the model published.
    ///
    /// The one arm a surface should render as a warning.
    Overridden {
        /// What the model author published.
        published: f64,
        /// What gglib puts on the wire instead.
        sending: f64,
    },
    /// The model names the key and gglib could not read its value.
    ///
    /// Never rendered as an override: gglib cannot tell what it displaced, and
    /// claiming otherwise is the mistake [ADR 0004] decision 3 forbids one
    /// layer up.
    ///
    /// [ADR 0004]: https://github.com/mmogr/gglib/blob/main/docs/adr/0004-observe-the-sampling-boundary.md
    Unreadable,
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
        "frequency_penalty" => "frequencyPenalty",
        "dynatemp_range" => "dynatempRange",
        "dynatemp_exponent" => "dynatempExponent",
        "top_n_sigma" => "topNSigma",
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
    let model_ctx = ModelSamplingContext::for_model(model);

    // An empty request layer: this explains the stored configuration, so there
    // are no per-request parameters to occupy the top rung.
    let (resolved, sources) = InferenceConfig::default().resolve_with_profile_explained(
        profile.map(|selected| &selected.config),
        model.inference_defaults.as_ref(),
        settings.inference_defaults.as_ref(),
        model_ctx,
    );

    // What gglib actually puts on the wire, read from the patch the request
    // pipeline merges into the body rather than from the struct fields. A
    // parameter missing from this map is one gglib names nowhere, which is
    // exactly the condition under which the model's own GGUF value survives to
    // the sampler.
    let patch = resolved.to_openai_json_patch();
    let published = ModelSamplingDefaults::from_metadata(&model.metadata)
        .compare_all(|field| patch.get(field).and_then(serde_json::Value::as_f64))
        .into_iter()
        .filter_map(|(field, verdict)| published_default(field, &verdict))
        .collect();

    SamplingExplanationDto {
        resolved,
        sources: sources
            .iter()
            .map(|(field, source)| provenance(field, source))
            .collect(),
        profile: profile.map(|selected| selected.name.clone()),
        is_reasoning: model_ctx.is_reasoning,
        trust_client_sampling: settings.trust_client_sampling.unwrap_or(false),
        published,
        defaults_origin: model.defaults_origin.map(DefaultsOriginDto::from),
    }
}

/// Translate one override verdict into its wire form.
///
/// `None` for [`SamplingOverride::NotPublished`], which is how a field with no
/// author recommendation stays out of the payload entirely.
fn published_default(
    field: &'static str,
    verdict: &SamplingOverride,
) -> Option<PublishedDefaultDto> {
    let (key, state) = match verdict {
        SamplingOverride::NotPublished => return None,
        SamplingOverride::Deferred { key, published } => (
            *key,
            PublishedStateDto::Deferred {
                published: *published,
            },
        ),
        SamplingOverride::Restated { key, published } => (
            *key,
            PublishedStateDto::Restated {
                published: *published,
            },
        ),
        SamplingOverride::Overridden {
            key,
            published,
            sending,
        } => (
            *key,
            PublishedStateDto::Overridden {
                published: *published,
                sending: *sending,
            },
        ),
        SamplingOverride::Unreadable { key, .. } => (*key, PublishedStateDto::Unreadable),
    };
    Some(PublishedDefaultDto {
        param: wire_key(field).to_owned(),
        key: key.to_owned(),
        state,
    })
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
    /// Config fields that deliberately carry no provenance.
    ///
    /// `seed` is request-scoped: no rung ever names one, so a provenance entry
    /// for it would read `unset by design` on every model forever. Listed
    /// rather than subtracted silently, so a field that loses its provenance by
    /// *accident* still fails the count below.
    const NO_PROVENANCE: [&str; 1] = ["seed"];

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
            dynatemp_range: Some(0.5),
            dynatemp_exponent: Some(1.0),
            top_n_sigma: Some(1.0),
            frequency_penalty: Some(0.4),
            seed: Some(100),
        };
        let keys = serde_json::to_value(&populated).unwrap();
        let keys = keys.as_object().expect("config serializes as an object");

        let dto = explain(&model(), &Settings::with_defaults(), None);
        assert_eq!(dto.sources.len(), keys.len() - NO_PROVENANCE.len());
        for excluded in NO_PROVENANCE {
            assert!(
                keys.contains_key(excluded),
                "{excluded} is no longer a config field; drop it from NO_PROVENANCE"
            );
            assert!(
                !dto.sources.iter().any(|e| e.param == excluded),
                "{excluded} must not carry provenance"
            );
        }
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

        assert_eq!(value["resolved"]["temperature"], json!(0.7), "{wire}");
        assert_eq!(value["isReasoning"], json!(false));
        assert_eq!(value["trustClientSampling"], json!(false));
        assert_eq!(value["profile"], json!(null));
        assert_eq!(
            value["sources"][0],
            json!({ "param": "temperature", "kind": "floor" })
        );
        // maxTokens is last in the canonical order, after the DRY block.
        assert_eq!(
            value["sources"][14],
            json!({ "param": "maxTokens", "kind": "unset" })
        );

        // ADR 0003: an untuned model resolves exactly one sampler, and every
        // other field is llama.cpp's to decide. Asserted on the wire because
        // this is the shape the frontend renders — a `null` here has to read
        // as "llama.cpp's default", never as "nothing configured this".
        assert_eq!(value["resolved"]["topP"], json!(null), "{wire}");
        assert_eq!(value["resolved"]["minP"], json!(null), "{wire}");
        assert_eq!(value["resolved"]["dryMultiplier"], json!(null), "{wire}");
        for entry in value["sources"].as_array().expect("sources is an array") {
            let param = entry["param"].as_str().unwrap();
            let expected = if param == "temperature" {
                "floor"
            } else {
                "unset"
            };
            assert_eq!(entry["kind"], json!(expected), "{param} in {wire}");
        }
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
                "frequencyPenalty",
                "dynatempRange",
                "dynatempExponent",
                "topNSigma",
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

        // The model's 1.5 was discarded because the profile claimed the
        // temperature, and since ADR 0003 the floor has no `presence_penalty`
        // to land on — so nothing is sent and llama.cpp's own default applies.
        assert_eq!(dto.resolved.presence_penalty, None);

        // The provenance must still name the coupling rule. Reporting `Unset`
        // here would say "nobody named a value" about a parameter a layer did
        // name and the rule deliberately passed over, which is the one thing
        // the resolved number alone can never explain. See the arm ordering in
        // `resolve_layers_with_sources`.
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

    // =========================================================================
    // What the model published
    // =========================================================================

    /// A model carrying `general.sampling.*` keys in its stored GGUF metadata.
    fn model_publishing(pairs: &[(&str, &str)]) -> Model {
        Model {
            metadata: pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            ..model()
        }
    }

    fn published_for<'a>(dto: &'a SamplingExplanationDto, param: &str) -> &'a PublishedDefaultDto {
        dto.published
            .iter()
            .find(|entry| entry.param == param)
            .unwrap_or_else(|| panic!("no published entry for {param} in {:?}", dto.published))
    }

    /// **The payload cost on every ordinary model.** Almost no GGUF carries
    /// these keys, and a `notPublished` entry per field would be five rows of
    /// nothing on every model in the library.
    #[test]
    fn a_model_publishing_nothing_carries_an_empty_list() {
        assert!(
            explain(&model(), &Settings::with_defaults(), None)
                .published
                .is_empty()
        );
    }

    /// The headline case: the model asks for one temperature and gglib sends
    /// another, so both numbers have to reach the client.
    #[test]
    fn an_overridden_value_carries_both_numbers() {
        let model = model_publishing(&[("general.sampling.temp", "0.33")]);
        let settings = Settings {
            inference_defaults: Some(InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            }),
            ..Settings::with_defaults()
        };

        let dto = explain(&model, &settings, None);
        let entry = published_for(&dto, "temperature");

        assert_eq!(entry.key, "general.sampling.temp");
        match entry.state {
            PublishedStateDto::Overridden { published, sending } => {
                assert!((published - 0.33).abs() < 1e-9, "{published}");
                assert!((sending - 1.0).abs() < 1e-6, "{sending}");
            }
            ref other => panic!("expected overridden, got {other:?}"),
        }
    }

    /// **The one the provenance column cannot express.** `unset` renders the
    /// same whether the model published a value or not, and the two mean
    /// opposite things about what the sampler will do.
    #[test]
    fn a_deferred_value_is_reported_beside_an_unset_provenance() {
        let model = model_publishing(&[("general.sampling.top_p", "0.71")]);

        let dto = explain(&model, &Settings::with_defaults(), None);

        assert_eq!(
            source_for(&dto, "topP").kind,
            ProvenanceKindDto::Unset,
            "guards the premise: gglib names nothing here"
        );
        assert_eq!(
            published_for(&dto, "topP").state,
            PublishedStateDto::Deferred { published: 0.71 }
        );
    }

    /// The `repeat_penalty` / `penalty_repeat` spelling gap is the backend's to
    /// close — no client should have to know the two names are one knob.
    #[test]
    fn the_gguf_key_is_carried_rather_than_left_to_the_client_to_derive() {
        let model = model_publishing(&[("general.sampling.penalty_repeat", "1.07")]);

        let dto = explain(&model, &Settings::with_defaults(), None);
        let entry = published_for(&dto, "repeatPenalty");

        assert_eq!(entry.key, "general.sampling.penalty_repeat");
        assert_ne!(entry.param, entry.key);
    }

    /// A value gglib cannot parse must reach the client as its own state, so
    /// the UI can render unknown rather than picking a side.
    #[test]
    fn an_unreadable_value_is_its_own_state() {
        let model = model_publishing(&[("general.sampling.temp", "warm")]);

        let dto = explain(&model, &Settings::with_defaults(), None);

        assert_eq!(
            published_for(&dto, "temperature").state,
            PublishedStateDto::Unreadable
        );
    }

    /// The wire contract the frontend is written against. Asserted on the
    /// serialized string for the reason the sources test gives: `to_value`
    /// widens `f32` and would report numbers no client receives.
    #[test]
    fn published_entries_serialize_with_a_flattened_state_tag() {
        let model = model_publishing(&[("general.sampling.temp", "0.33")]);
        let settings = Settings {
            inference_defaults: Some(InferenceConfig {
                temperature: Some(1.0),
                ..InferenceConfig::default()
            }),
            ..Settings::with_defaults()
        };

        let json = serde_json::to_string(&explain(&model, &settings, None)).expect("serializes");

        assert!(json.contains(r#""param":"temperature""#), "{json}");
        assert!(json.contains(r#""key":"general.sampling.temp""#), "{json}");
        assert!(json.contains(r#""state":"overridden""#), "{json}");
        assert!(json.contains(r#""published":0.33"#), "{json}");
    }

    /// A field no model can reach must never appear here, however the metadata
    /// spells it — `presence_penalty` and `dry_multiplier` have no GGUF key.
    #[test]
    fn a_field_with_no_gguf_key_never_appears() {
        let model = model_publishing(&[
            ("general.sampling.presence_penalty", "0.0"),
            ("general.sampling.dry_multiplier", "0.8"),
        ]);

        assert!(
            explain(&model, &Settings::with_defaults(), None)
                .published
                .is_empty()
        );
    }
}
