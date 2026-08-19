//! Tests for [`super`] — the wire form of a resolved sampling config.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::Utc;
use gglib_core::ModelCapabilities;
use gglib_core::domain::{DefaultsOrigin, ReasoningEffort};
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
        template_caps: None,
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
            "reasoningEffort",
            "reasoningBudgetTokens",
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
        DefaultsOrigin::AutoDetected,
        DefaultsOrigin::Published,
        DefaultsOrigin::Measured,
    ] {
        let stored = Model {
            dialect_spec: None,
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
                serde_json::to_value(crate::types::ModelDetailDto::from_model(
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

// =========================================================================
// A suppressed reasoning effort (ADR 0007 stage 5b)
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

fn high_effort_profile() -> InferenceProfile {
    profile(
        "high",
        InferenceConfig {
            reasoning_effort: Some(ReasoningEffort::High),
            reasoning_budget_tokens: Some(16384),
            ..InferenceConfig::default()
        },
    )
}

/// **The whole point of the field.** With only the table, a client can see
/// `kind: suppressedByTemplate` and `resolved.reasoningEffort: null` — enough
/// to say *something* was dropped, and not enough to say what or whose. Both
/// halves are destroyed by the gate, so they arrive here or nowhere.
#[test]
fn a_suppressed_effort_carries_the_level_and_the_rung_that_asked_for_it() {
    let dto = explain(
        &model_whose_template(Some(false)),
        &Settings::with_defaults(),
        Some(&high_effort_profile()),
    );

    let suppressed = dto.effort_suppressed.expect("the suppression is reported");
    assert_eq!(suppressed.level, ReasoningEffort::High);
    assert_eq!(suppressed.layer, Some(SamplingLayerDto::Profile));
}

/// The table and the new field have to tell one story: `resolved` shows what
/// would be sent (nothing), and the provenance names the suppression rather
/// than the rung whose value did not survive.
#[test]
fn the_table_agrees_with_the_suppression_it_reports() {
    let dto = explain(
        &model_whose_template(Some(false)),
        &Settings::with_defaults(),
        Some(&high_effort_profile()),
    );

    assert_eq!(dto.resolved.reasoning_effort, None);
    assert_eq!(
        source_for(&dto, "reasoningEffort").kind,
        ProvenanceKindDto::SuppressedByTemplate
    );
    assert_eq!(
        source_for(&dto, "reasoningEffort").layer,
        None,
        "the rung is gone from the table by design; it is on effort_suppressed"
    );
}

/// The budget is enforced by llama.cpp's own sampler, not by a template, so it
/// survives on exactly the model where the effort does not. A client that
/// greyed out both controls on a `no` would be wrong about the half that works.
#[test]
fn the_budget_survives_the_template_that_ignores_the_effort() {
    let dto = explain(
        &model_whose_template(Some(false)),
        &Settings::with_defaults(),
        Some(&high_effort_profile()),
    );

    assert_eq!(dto.resolved.reasoning_budget_tokens, Some(16384));
    assert_eq!(
        source_for(&dto, "reasoningBudgetTokens").layer,
        Some(SamplingLayerDto::Profile)
    );
}

/// **Unknown never gates.** Caps are read from `/props` while a model runs, so
/// a model nobody has launched has none — and reporting a suppression there
/// would tell an operator their profile is inert on evidence that does not
/// exist.
#[test]
fn a_model_whose_template_was_never_observed_suppresses_nothing() {
    for m in [
        model(),
        model_whose_template(None),
        model_whose_template(Some(true)),
    ] {
        let dto = explain(&m, &Settings::with_defaults(), Some(&high_effort_profile()));

        assert_eq!(dto.effort_suppressed, None, "{:?}", m.template_caps);
        assert_eq!(dto.resolved.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(
            source_for(&dto, "reasoningEffort").layer,
            Some(SamplingLayerDto::Profile)
        );
    }
}

/// A suppressing model that nothing configured an effort for has nothing to
/// report — the field must not become a permanent fixture on such rows.
#[test]
fn a_suppressing_model_with_no_effort_resolved_reports_nothing() {
    let dto = explain(
        &model_whose_template(Some(false)),
        &Settings::with_defaults(),
        None,
    );
    assert_eq!(dto.effort_suppressed, None);
}

/// The wire contract, and the omission that keeps it off every ordinary
/// payload: a `null` here on every model in the library would be a field a
/// client has to check before it can learn nothing.
#[test]
fn the_suppression_is_absent_from_the_payload_unless_it_happened() {
    let quiet = serde_json::to_string(&explain(
        &model(),
        &Settings::with_defaults(),
        Some(&high_effort_profile()),
    ))
    .expect("serializes");
    assert!(!quiet.contains("effortSuppressed"), "{quiet}");

    let loud = serde_json::to_string(&explain(
        &model_whose_template(Some(false)),
        &Settings::with_defaults(),
        Some(&high_effort_profile()),
    ))
    .expect("serializes");
    assert!(loud.contains(r#""effortSuppressed":{"#), "{loud}");
    assert!(loud.contains(r#""level":"high""#), "{loud}");
    assert!(loud.contains(r#""layer":"profile""#), "{loud}");
    assert!(loud.contains(r#""kind":"suppressedByTemplate""#), "{loud}");
}
