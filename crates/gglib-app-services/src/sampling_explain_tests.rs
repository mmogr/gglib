//! Tests for [`super`] — the wire form of a resolved sampling config.

use std::path::PathBuf;

use chrono::Utc;
use gglib_core::NewModel;
use gglib_core::domain::{DefaultsOrigin, ReasoningEffort};
use serde_json::json;

use super::*;

/// A model with nothing sampling-relevant set. `Model` has no `Default`,
/// so the variants below build on this with struct-update syntax.
fn model() -> Model {
    let path = PathBuf::from("/models/test.gguf");
    Model::stored(
        1,
        &NewModel::new("test-model".to_owned(), path, Some(7.0), Utc::now()),
    )
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

/// A refusal the caller can fix, worded by the one message every profile
/// lookup answers with, so a typo reads the same in a terminal and over HTTP.
#[test]
fn an_unknown_profile_is_refused_with_the_shared_message() {
    let profiles = vec![profile("coding", InferenceConfig::default())];

    for stored in [Some(profiles.as_slice()), None] {
        let Err(GuiError::ValidationFailed(message)) = find_profile("codign", stored) else {
            panic!("an unknown profile is a validation failure");
        };
        assert_eq!(
            message,
            not_found_message("codign", stored.unwrap_or_default())
        );
    }
}

#[test]
fn a_layer_index_resolves_to_its_rung_name() {
    let entry = provenance("temperature", ParamSource::Layer(2));
    assert_eq!(entry.kind, ProvenanceKindDto::Layer);
    assert_eq!(entry.layer, Some(SamplingLayerDto::ModelUserSet));
}

#[test]
fn the_unsent_variants_stay_distinguishable_and_carry_no_layer() {
    let coupled = provenance("min_p", ParamSource::Coupled);
    assert_eq!(coupled.kind, ProvenanceKindDto::Coupled);
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
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(4096),
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
fn serializes_camel_case_with_the_layer_omitted_for_unset() {
    let dto = explain(&model(), &Settings::with_defaults(), None);
    let wire = serde_json::to_string(&dto).unwrap();
    let value: serde_json::Value = serde_json::from_str(&wire).unwrap();

    assert_eq!(value["resolved"]["temperature"], json!(null), "{wire}");
    assert_eq!(value.get("isReasoning"), None, "{wire}");
    assert_eq!(value["trustClientSampling"], json!(false));
    assert_eq!(value["profile"], json!(null));
    assert_eq!(
        value["sources"][0],
        json!({ "param": "temperature", "kind": "unset" })
    );
    // maxTokens is last in the canonical order, after the DRY block.
    assert_eq!(
        value["sources"][14],
        json!({ "param": "maxTokens", "kind": "unset" })
    );

    // ADR 0003 and ADR 0022: an untuned model resolves no sampler, and
    // every field is llama.cpp's to decide. Asserted on the wire because
    // this is the shape the frontend renders — a `null` here has to read
    // as "llama.cpp's default", never as "nothing configured this".
    assert_eq!(value["resolved"]["topP"], json!(null), "{wire}");
    assert_eq!(value["resolved"]["minP"], json!(null), "{wire}");
    assert_eq!(value["resolved"]["dryMultiplier"], json!(null), "{wire}");
    for entry in value["sources"].as_array().expect("sources is an array") {
        let param = entry["param"].as_str().unwrap();
        assert_eq!(entry["kind"], json!("unset"), "{param} in {wire}");
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
            "reasoningEffort",
            "reasoningBudgetTokens",
        ]
    );
}

/// No floor beneath the ladder (ADR 0022): an untagged model with nothing
/// stored resolves nothing, and every field reads as unset.
#[test]
fn a_model_with_nothing_stored_resolves_nothing() {
    let dto = explain(&model(), &Settings::with_defaults(), None);

    assert_eq!(dto.resolved, InferenceConfig::default());
    assert!(
        dto.sources
            .iter()
            .all(|entry| entry.kind == ProvenanceKindDto::Unset),
        "{:?}",
        dto.sources
    );
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
fn an_unreviewed_recipe_ranks_below_global_settings() {
    let settings = Settings {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.4),
            ..Default::default()
        }),
        ..Settings::with_defaults()
    };
    let auto = Model {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(1.0),
            ..Default::default()
        }),
        defaults_origin: Some(DefaultsOrigin::Published),
        ..model()
    };

    let dto = explain(&auto, &settings, None);

    assert_eq!(dto.resolved.temperature, Some(0.4));
    assert_eq!(
        source_for(&dto, "temperature").layer,
        Some(SamplingLayerDto::Global)
    );

    let user = Model {
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

/// Every surface that reports this column spells its values the same way.
///
/// `Model.defaults_origin` is one value in one database column, and three
/// separately-declared DTOs serialize it across four routes: [`GuiModel`] for
/// the library list, [`ModelDetailDto`] for the inspector, and
/// [`SamplingExplanationDto`] here. The last used to re-spell it through a DTO
/// that differed from the domain enum in nothing but its casing, so a client
/// holding two replies for the same stored model read `auto_detected` from one
/// and `autoDetected` from the other, and could not compare them without first
/// knowing which route each had come from.
///
/// Each surface is checked against the domain enum's own [`Display`], not
/// against the other surfaces. Comparing them to each other looks equivalent
/// and is not: `Value::Index` yields `Null` for a key that is absent, so
/// mutual comparison holds just as well when *every* surface has dropped the
/// field as when they agree on it — and it pins no spelling at all, leaving a
/// change that re-camelCased all three green.
///
/// [`Display`]: std::fmt::Display
#[test]
fn every_surface_spells_an_origin_the_same_way() {
    for origin in [
        DefaultsOrigin::User,
        DefaultsOrigin::Published,
        DefaultsOrigin::Measured,
    ] {
        let stored = Model {
            defaults_origin: Some(origin),
            ..model()
        };
        let expected = serde_json::Value::String(origin.to_string());

        let surfaces = [
            (
                "the explain endpoint",
                serde_json::to_value(explain(&stored, &Settings::with_defaults(), None)).unwrap(),
            ),
            (
                "the library list",
                serde_json::to_value(crate::types::GuiModel::from_domain(stored.clone())).unwrap(),
            ),
            (
                "the model inspector",
                serde_json::to_value(gglib_core::domain::ModelDetailDto::from_model(
                    stored.clone(),
                    false,
                    None,
                ))
                .unwrap(),
            ),
        ];

        for (surface, payload) in surfaces {
            assert_eq!(
                payload["defaultsOrigin"], expected,
                "{surface} does not spell {origin:?} the way the domain enum does"
            );
        }
    }
}

/// The tag supplies the `reasoning` recipe beneath global settings, with
/// nothing stored on the row.
#[test]
fn the_reasoning_tag_supplies_the_reasoning_recipe() {
    let reasoning = Model {
        tags: vec!["Reasoning".to_owned()],
        ..model()
    };

    let dto = explain(&reasoning, &Settings::with_defaults(), None);

    assert_eq!(dto.resolved, InferenceConfig::reasoning_profile());
    assert_eq!(
        source_for(&dto, "temperature").layer,
        Some(SamplingLayerDto::ModelAutoDetected)
    );
    assert_eq!(dto.defaults_origin, None);
}

/// A layer that claims `temperature` passes over lower rungs for the trio
/// tuned against it — the case a bare value cannot explain.
#[test]
fn a_claimed_temperature_passes_over_the_trio_beneath_it() {
    let coding = profile(
        "coding",
        InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        },
    );
    let tuned = Model {
        inference_defaults: Some(InferenceConfig {
            presence_penalty: Some(1.5),
            ..Default::default()
        }),
        defaults_origin: Some(DefaultsOrigin::User),
        ..model()
    };

    let dto = explain(&tuned, &Settings::with_defaults(), Some(&coding));

    // The model's 1.5 was discarded because the profile claimed the
    // temperature, and there is no floor — so nothing is sent and
    // llama.cpp's own default applies.
    assert_eq!(dto.resolved.presence_penalty, None);

    // The provenance must still name the coupling rule. Reporting `Unset`
    // here would say "nobody named a value" about a parameter a layer did
    // name and the rule deliberately passed over, which is the one thing
    // the resolved number alone can never explain. See the arm ordering in
    // `resolve_layers_with_sources`.
    assert_eq!(
        source_for(&dto, "presencePenalty").kind,
        ProvenanceKindDto::Coupled
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

// =========================================================================
// A template's caps are read, never used to edit the resolution (ADR 0007)
// =========================================================================

use gglib_core::domain::TemplateCaps;

/// A model whose last launch reported this reading for the effort variable.
fn model_whose_template(supports_reasoning_effort: Option<bool>) -> Model {
    Model {
        template_caps: Some(TemplateCaps {
            supports_reasoning_effort,
            ..TemplateCaps::default()
        }),
        ..model()
    }
}

/// A resolved level is sent whatever the template says about it; a template
/// that does not read it ignores it. So every caps reading, `false` included,
/// explains the same level from the same rung, and the budget beside it.
#[test]
fn every_template_reading_explains_the_same_effort() {
    let high = profile(
        "high",
        InferenceConfig {
            reasoning_effort: Some(ReasoningEffort::High),
            reasoning_budget_tokens: Some(16384),
            ..InferenceConfig::default()
        },
    );
    for m in [
        model(),
        model_whose_template(None),
        model_whose_template(Some(true)),
        model_whose_template(Some(false)),
    ] {
        let dto = explain(&m, &Settings::with_defaults(), Some(&high));

        assert_eq!(
            dto.resolved.reasoning_effort,
            Some(ReasoningEffort::High),
            "{:?}",
            m.template_caps
        );
        assert_eq!(
            source_for(&dto, "reasoningEffort").layer,
            Some(SamplingLayerDto::Profile)
        );
        assert_eq!(dto.resolved.reasoning_budget_tokens, Some(16384));
    }
}
