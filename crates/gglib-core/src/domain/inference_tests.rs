//! Tests for the inference-config domain types.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;

// =========================================================================
// seed
// =========================================================================

/// A seed is request-scoped. No floor may name one, or every untuned
/// request in the installation would decode identically.
#[test]
fn no_floor_names_a_seed() {
    assert_eq!(InferenceConfig::with_hardcoded_defaults().seed, None);
    assert_eq!(InferenceConfig::reasoning_floor().seed, None);
    assert_eq!(InferenceConfig::reasoning_profile().seed, None);
}

/// It still has to reach the wire, which is the whole reason it lives in
/// this struct rather than beside the hierarchy.
#[test]
fn a_seed_reaches_the_request_body() {
    let config = InferenceConfig {
        seed: Some(100),
        ..InferenceConfig::default()
    };
    let patch = config.to_openai_json_patch();

    assert_eq!(
        patch.get("seed").and_then(serde_json::Value::as_u64),
        Some(100)
    );
}

/// An unseeded config must emit no key at all — llama.cpp then draws its
/// own, and a `null` or sentinel would be a different request.
#[test]
fn an_unseeded_config_emits_no_seed_key() {
    assert!(
        !InferenceConfig::with_hardcoded_defaults()
            .to_openai_json_patch()
            .contains_key("seed")
    );
}

/// Seeds resolve like any uncoupled field: the first layer that names one
/// wins, and naming one does not claim the coupled trio.
#[test]
fn a_seed_resolves_without_claiming_the_coupled_trio() {
    let top = InferenceConfig {
        seed: Some(100),
        ..InferenceConfig::default()
    };
    let below = InferenceConfig {
        temperature: Some(0.6),
        presence_penalty: Some(1.5),
        ..InferenceConfig::default()
    };

    let (resolved, _) = InferenceConfig::resolve_layers_with_sources(
        &[Some(&top), Some(&below)],
        &InferenceConfig::with_hardcoded_defaults(),
    );

    assert_eq!(resolved.seed, Some(100));
    assert_eq!(
        resolved.presence_penalty,
        Some(1.5),
        "a seed must not hijack the trio the way a temperature does"
    );
    assert_eq!(resolved.temperature, Some(0.6));
}

/// llama.cpp spells "random" two ways and this type spells it as absence.
/// Carrying a sentinel would give one state two representations and make
/// `seed.is_some()` stop meaning "reproducible".
#[test]
fn the_random_seed_sentinels_normalise_to_absence() {
    for raw in ["-1", "4294967295"] {
        let body: serde_json::Value =
            serde_json::from_str(&format!(r#"{{"seed": {raw}}}"#)).unwrap();
        let (cfg, issues) = InferenceConfig::extract_client_sampling(&body);

        assert_eq!(cfg.seed, None, "{raw}");
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, FieldIssue::Normalised { field: "seed", .. })),
            "{raw} should be reported as normalised, not dropped silently"
        );
    }
}

#[test]
fn a_client_seed_is_read_from_the_request_body() {
    let body: serde_json::Value = serde_json::from_str(r#"{"seed": 12345}"#).unwrap();
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&body);

    assert_eq!(cfg.seed, Some(12345));
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn a_seed_that_is_not_an_integer_is_rejected_rather_than_ignored() {
    let body: serde_json::Value = serde_json::from_str(r#"{"seed": "abc"}"#).unwrap();
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&body);

    assert_eq!(cfg.seed, None);
    assert!(
        issues
            .iter()
            .any(|i| matches!(i, FieldIssue::Rejected { field: "seed", .. })),
        "{issues:?}"
    );
}

#[test]
fn test_default_is_all_none() {
    let config = InferenceConfig::default();
    assert!(config.temperature.is_none());
    assert!(config.top_p.is_none());
    assert!(config.top_k.is_none());
    assert!(config.max_tokens.is_none());
    assert!(config.repeat_penalty.is_none());
    assert!(config.presence_penalty.is_none());
    assert!(config.min_p.is_none());
}

#[test]
fn test_merge_with_prefers_self() {
    let mut request = InferenceConfig {
        temperature: Some(0.8),
        top_p: None,
        ..Default::default()
    };

    let model_defaults = InferenceConfig {
        temperature: Some(0.5),
        top_p: Some(0.9),
        top_k: Some(50),
        ..Default::default()
    };

    request.merge_with(&model_defaults);

    assert_eq!(request.temperature, Some(0.8)); // Request wins
    assert_eq!(request.top_p, Some(0.9)); // Fallback to model
    assert_eq!(request.top_k, Some(50)); // Fallback to model
    assert!(request.max_tokens.is_none()); // Still None
}

/// The reasoning floor differs from the hardcoded floor in exactly two
/// fields, both class-specific: a real anti-repetition guard where the
/// neutral floor has none, and min-p disabled per Qwen3.6's guidance
/// where the neutral floor matches llama.cpp.
#[test]
fn test_reasoning_floor_differs_only_in_presence_penalty_and_min_p() {
    let neutral = InferenceConfig::with_hardcoded_defaults();
    let reasoning = InferenceConfig::reasoning_floor();

    assert_eq!(reasoning.presence_penalty, Some(1.0));
    assert_ne!(reasoning.presence_penalty, neutral.presence_penalty);

    assert_eq!(reasoning.min_p, Some(0.0));
    assert_ne!(reasoning.min_p, neutral.min_p);

    assert_eq!(reasoning.temperature, neutral.temperature);
    assert_eq!(reasoning.top_p, neutral.top_p);
    assert_eq!(reasoning.top_k, neutral.top_k);
    assert_eq!(reasoning.max_tokens, neutral.max_tokens);
    assert_eq!(reasoning.repeat_penalty, neutral.repeat_penalty);
}

/// If nothing in the stack ever declares a temperature, nothing has been
/// "tuned" against anything — the coupled set must gap-fill exactly like
/// any other parameter, from whichever layer sets it first, rather than
/// jump straight to the floor.
#[test]
fn test_coupled_trio_gap_fills_normally_when_no_layer_sets_temperature() {
    let profile = InferenceConfig {
        presence_penalty: Some(0.3),
        ..Default::default()
    };
    let model = InferenceConfig {
        presence_penalty: Some(0.5),
        repeat_penalty: Some(1.2),
        ..Default::default()
    };

    let resolved = InferenceConfig::default().resolve_with_profile(
        Some(&profile),
        Some(&model),
        None,
        ModelSamplingContext::default(),
    );

    assert_eq!(resolved.temperature, Some(0.7), "hardcoded fallback");
    assert_eq!(
        resolved.presence_penalty,
        Some(0.3),
        "profile's own value, not suppressed just because no layer set a temperature"
    );
    assert_eq!(
        resolved.repeat_penalty,
        Some(1.2),
        "model fills in what the profile left unset"
    );
}

/// An unset `max_tokens` must not be written into the request body.
///
/// This used to check a second route as well — a `-n` flag on the launch
/// command line, which is the more dangerous of the two because it sets
/// `global_params.n_predict`, a server-wide ceiling overriding even a
/// per-request `-1`. ADR 0003 deleted `to_cli_args`, so that route no
/// longer exists for any parameter and there is nothing left to assert
/// about it: the guarantee moved from a test to the type system.
#[test]
fn test_unset_max_tokens_is_not_written_into_the_body() {
    let resolved = InferenceConfig::default().resolve_with_defaults(
        None,
        None,
        ModelSamplingContext::default(),
    );

    assert!(
        !resolved.to_openai_json_patch().contains_key("max_tokens"),
        "an unset max_tokens must not be written into the request body"
    );
}

/// This change removed the *fallback*, not the parameter.
#[test]
fn test_explicit_max_tokens_is_still_forwarded() {
    let resolved = InferenceConfig {
        max_tokens: Some(512),
        ..Default::default()
    }
    .resolve_with_defaults(None, None, ModelSamplingContext::default());

    assert_eq!(resolved.max_tokens, Some(512));
    assert_eq!(
        resolved.to_openai_json_patch().get("max_tokens"),
        Some(&serde_json::json!(512))
    );
}

/// The floor asserts exactly one parameter, and it is the one ADR 0003
/// measured as diverging from upstream.
///
/// Written as an exhaustive field-by-field check rather than an equality
/// against a literal so that adding a value back is a *failure with a
/// name*, not a diff someone re-blesses. Every `None` here is a field
/// llama.cpp supplies; setting one again overrides whatever upstream
/// chooses next, which is #739's failure mode.
#[test]
fn the_floor_asserts_only_the_value_that_diverges_from_upstream() {
    let floor = InferenceConfig::with_hardcoded_defaults();

    assert_eq!(
        floor.temperature,
        Some(0.7),
        "the one genuine policy choice; upstream's is 0.8"
    );

    for (field, value) in [
        ("top_p", floor.top_p),
        ("min_p", floor.min_p),
        ("repeat_penalty", floor.repeat_penalty),
        ("presence_penalty", floor.presence_penalty),
        ("dry_multiplier", floor.dry_multiplier),
        ("dry_base", floor.dry_base),
    ] {
        assert_eq!(value, None, "{field} is deferred to llama.cpp (ADR 0003)");
    }
    assert_eq!(floor.top_k, None, "top_k is deferred to llama.cpp");
    assert_eq!(
        floor.max_tokens, None,
        "max_tokens has no fallback by design"
    );
    assert_eq!(floor.dry_allowed_length, None);
    assert_eq!(floor.dry_penalty_last_n, None);
}

/// The non-uniformity ADR 0003 decision 3 called out: `min_p` is asserted
/// for reasoning models and deferred for everything else. Pinned because
/// it reads like a bug when you diff two requests.
#[test]
fn min_p_is_asserted_for_reasoning_models_and_deferred_for_the_rest() {
    assert_eq!(InferenceConfig::reasoning_floor().min_p, Some(0.0));
    assert_eq!(InferenceConfig::with_hardcoded_defaults().min_p, None);

    assert_eq!(
        InferenceConfig::reasoning_floor().presence_penalty,
        Some(1.0)
    );
    assert_eq!(
        InferenceConfig::with_hardcoded_defaults().presence_penalty,
        None
    );
}

/// The whole point of the deferral: an untuned request names one sampler,
/// not seven, so llama.cpp's own defaults apply to the rest.
#[test]
fn an_untuned_request_body_carries_only_the_temperature() {
    let resolved = InferenceConfig::default().resolve_with_defaults(
        None,
        None,
        ModelSamplingContext::default(),
    );
    let patch = resolved.to_openai_json_patch();

    assert_eq!(
        patch.keys().collect::<Vec<_>>(),
        vec!["temperature"],
        "anything else here is gglib overriding an upstream default: {patch:?}"
    );
}

#[test]
fn test_reasoning_profile() {
    let profile = InferenceConfig::reasoning_profile();
    assert_eq!(profile.temperature, Some(1.0));
    assert_eq!(profile.top_p, Some(0.95));
    assert_eq!(profile.top_k, Some(20));
    assert_eq!(profile.max_tokens, Some(8192));
    assert_eq!(profile.repeat_penalty, Some(1.0));
    assert_eq!(profile.presence_penalty, Some(1.5));
    assert_eq!(profile.min_p, Some(0.0));
}

#[test]
fn test_serialization() {
    let config = InferenceConfig {
        temperature: Some(0.7),
        top_p: Some(0.9),
        top_k: None,
        max_tokens: Some(1024),
        repeat_penalty: None,
        presence_penalty: None,
        min_p: None,
        // A set and an unset DRY field, so the round-trip covers both the
        // camelCase rename and the `Option` shape for the new parameters.
        dry_multiplier: Some(0.8),
        dry_base: None,
        dry_allowed_length: Some(2),
        dry_penalty_last_n: Some(-1),
        // Same coverage shape for the entropy-adaptive fields.
        dynatemp_range: Some(0.5),
        dynatemp_exponent: None,
        top_n_sigma: Some(1.5),
        frequency_penalty: Some(0.4),
        seed: Some(100),
        // And again for the reasoning pair, whose set/unset shape has an extra
        // wrinkle: the effort level is an enum, so a rename bug shows up as a
        // deserialize failure rather than as a wrong number.
        reasoning_effort: Some(ReasoningEffort::XHigh),
        reasoning_budget_tokens: None,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: InferenceConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(config, deserialized);
}

#[test]
fn test_camel_to_snake() {
    assert_eq!(camel_to_snake("temperature"), "temperature");
    assert_eq!(camel_to_snake("topP"), "top_p");
    assert_eq!(camel_to_snake("topK"), "top_k");
    assert_eq!(camel_to_snake("maxTokens"), "max_tokens");
    assert_eq!(camel_to_snake("repeatPenalty"), "repeat_penalty");
    assert_eq!(camel_to_snake("presencePenalty"), "presence_penalty");
    assert_eq!(camel_to_snake("minP"), "min_p");
    assert_eq!(camel_to_snake("dynatempRange"), "dynatemp_range");
    assert_eq!(camel_to_snake("dynatempExponent"), "dynatemp_exponent");
    assert_eq!(camel_to_snake("topNSigma"), "top_n_sigma");
}

#[test]
fn test_resolve_with_defaults_hierarchy() {
    let request = InferenceConfig {
        temperature: Some(0.9),
        ..Default::default()
    };
    let model = InferenceConfig {
        temperature: Some(0.5),
        top_p: Some(0.8),
        ..Default::default()
    };
    let global = InferenceConfig {
        top_k: Some(10),
        ..Default::default()
    };

    let resolved =
        request.resolve_with_defaults(Some(&model), Some(&global), ModelSamplingContext::default());

    assert_eq!(resolved.temperature, Some(0.9)); // request wins
    assert_eq!(resolved.top_p, Some(0.8)); // model fills in
    assert_eq!(resolved.top_k, Some(10)); // global fills in
    assert_eq!(resolved.max_tokens, None); // no layer sets it; stays unset
    // Deferred to llama.cpp since ADR 0003 — no layer named it and the
    // floor no longer restates upstream's own 1.0.
    assert_eq!(resolved.repeat_penalty, None);
}

#[test]
fn test_resolve_with_defaults_no_layers() {
    let base = InferenceConfig::default();
    let resolved = base.resolve_with_defaults(None, None, ModelSamplingContext::default());
    // Should equal hardcoded defaults
    assert_eq!(resolved, InferenceConfig::with_hardcoded_defaults());
}

/// Every layer contributes exactly one distinguishable parameter, so a
/// single assertion set pins the whole precedence ladder.
#[test]
fn test_resolve_with_profile_full_precedence_ladder() {
    let request = InferenceConfig {
        temperature: Some(0.9),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.2),
        top_p: Some(0.85),
        ..Default::default()
    };
    let model = InferenceConfig {
        temperature: Some(0.5),
        top_p: Some(0.8),
        presence_penalty: Some(1.5),
        ..Default::default()
    };
    let global = InferenceConfig {
        top_k: Some(10),
        ..Default::default()
    };

    let resolved = request.resolve_with_profile(
        Some(&profile),
        Some(&model),
        Some(&global),
        ModelSamplingContext::default(),
    );

    assert_eq!(resolved.temperature, Some(0.9)); // request beats profile
    assert_eq!(resolved.top_p, Some(0.85)); // profile beats model
    assert_eq!(resolved.top_k, Some(10)); // global fills in
    // The request claimed the temperature, so the model's 1.5 — tuned for
    // its own 0.5 — must not fall through. Nothing is sent instead: the
    // neutral floor used to restate upstream's 0.0 here and ADR 0003
    // deferred it, so llama.cpp supplies the same number it always did.
    // The suppression is still visible in the provenance, which reports
    // `FloorCoupled` rather than a plain absence.
    assert_eq!(resolved.presence_penalty, None);
    assert_eq!(resolved.repeat_penalty, None);
}

/// The invariant that makes one global profile safe across differing
/// architectures: parameters the profile leaves `None` still resolve from
/// the model, so selecting a profile cannot erase per-model tuning.
///
/// The exception is parameters tuned against temperature — see
/// [`test_profile_temperature_does_not_inherit_model_penalties`].
#[test]
fn test_sparse_profile_does_not_erase_model_defaults() {
    let profile = InferenceConfig {
        temperature: Some(0.2),
        ..Default::default()
    };
    let model = InferenceConfig::reasoning_profile();

    let resolved = InferenceConfig::default().resolve_with_profile(
        Some(&profile),
        Some(&model),
        None,
        ModelSamplingContext::default(),
    );

    assert_eq!(resolved.temperature, Some(0.2)); // the profile's one opinion
    // Untuned parameters the profile stayed silent about still come from
    // the model — this is what keeps one profile safe across architectures.
    assert_eq!(resolved.top_k, model.top_k);
    assert_eq!(resolved.top_p, model.top_p);
    assert_eq!(resolved.max_tokens, model.max_tokens);
}

/// Regression for #621: a sparse profile that lowers the temperature must
/// not inherit penalties the model tuned for a much broader distribution.
///
/// `reasoning_profile()` pairs `temperature 1.0` with `presence_penalty
/// 1.5` deliberately. Applying that 1.5 to a near-greedy `temperature 0.2`
/// request is a recipe no layer ever intended, and it reached production on
/// every `:coding` request.
///
/// The #621 fix originally floored `presence_penalty` to the universal
/// neutral `0.0` here — correct in that it stopped the wrong transplant,
/// but it also zeroed the model's only anti-repetition guard on a
/// reasoning model, which is a second failure mode of its own (see the
/// 2026-07-31 incident this floor was added for). `model_is_reasoning:
/// true` selects [`InferenceConfig::reasoning_floor`] instead, which keeps
/// a real, non-tuned-for-0.2 guard in place.
#[test]
fn test_profile_temperature_does_not_inherit_model_penalties() {
    let model = InferenceConfig::reasoning_profile();
    assert_eq!(model.temperature, Some(1.0), "guards the premise");
    assert_eq!(model.presence_penalty, Some(1.5), "guards the premise");

    // Mirrors the shipped `coding` profile.
    let profile = InferenceConfig {
        temperature: Some(0.2),
        top_p: Some(0.95),
        top_k: Some(20),
        max_tokens: Some(8192),
        min_p: Some(0.05),
        ..Default::default()
    };

    let resolved = InferenceConfig::default().resolve_with_profile(
        Some(&profile),
        Some(&model),
        None,
        ModelSamplingContext {
            is_reasoning: true,
            ..Default::default()
        },
    );

    assert_eq!(resolved.temperature, Some(0.2));
    assert_eq!(
        resolved.presence_penalty,
        Some(1.0),
        "must not inherit 1.5, but must not go silently to zero either"
    );
    assert_eq!(
        resolved.repeat_penalty, None,
        "not the model's 1.2, and no longer restated at the floor either"
    );
    assert_eq!(resolved.min_p, Some(0.05), "the profile's own value stands");
}

/// The coupling is directional: a layer that supplies a temperature *and*
/// its penalties still contributes them together, so a coherent recipe
/// stored on a model is untouched when nothing above it sets a temperature.
#[test]
fn test_model_recipe_applies_intact_when_no_layer_sets_temperature() {
    let model = InferenceConfig::reasoning_profile();
    // A profile with opinions only about untuned parameters.
    let profile = InferenceConfig {
        top_k: Some(64),
        ..Default::default()
    };

    let resolved = InferenceConfig::default().resolve_with_profile(
        Some(&profile),
        Some(&model),
        None,
        ModelSamplingContext {
            is_reasoning: true,
            ..Default::default()
        },
    );

    assert_eq!(resolved.temperature, model.temperature);
    assert_eq!(resolved.presence_penalty, model.presence_penalty);
    assert_eq!(resolved.repeat_penalty, model.repeat_penalty);
    assert_eq!(resolved.top_k, Some(64)); // profile still wins where it spoke
}

/// `resolve_with_defaults` delegates to `resolve_with_profile`, so the two
/// must stay observably identical when no profile is selected.
#[test]
fn test_resolve_with_defaults_matches_profile_form_with_no_profile() {
    let request = InferenceConfig {
        temperature: Some(0.9),
        ..Default::default()
    };
    let model = InferenceConfig::reasoning_profile();
    let global = InferenceConfig {
        top_k: Some(10),
        ..Default::default()
    };

    assert_eq!(
        request.clone().resolve_with_defaults(
            Some(&model),
            Some(&global),
            ModelSamplingContext::default()
        ),
        request.resolve_with_profile(
            None,
            Some(&model),
            Some(&global),
            ModelSamplingContext::default()
        ),
    );
}

// ── Provenance agrees with the values ─────────────────────────────────

/// Assert, for every field, that the reported source actually accounts for
/// the resolved value.
///
/// This is the invariant that makes the two impossible to drift apart, and
/// it is the check that would have caught the `describe_provenance`
/// divergence this API replaced: a `Layer(i)` claim is only true if that
/// layer really carries the resolved value.
/// Field name, the value that resolved, and how to read that field off any
/// layer — enough to check a reported source against reality.
type FieldCheck = (
    &'static str,
    Option<f32>,
    fn(&InferenceConfig) -> Option<f32>,
);

#[track_caller]
fn assert_sources_explain_values(layers: &[Option<&InferenceConfig>], floor: &InferenceConfig) {
    let (resolved, sources) = InferenceConfig::resolve_layers_with_sources(layers, floor);

    let checks: [FieldCheck; 5] = [
        ("temperature", resolved.temperature, |c| c.temperature),
        ("top_p", resolved.top_p, |c| c.top_p),
        ("presence_penalty", resolved.presence_penalty, |c| {
            c.presence_penalty
        }),
        ("repeat_penalty", resolved.repeat_penalty, |c| {
            c.repeat_penalty
        }),
        ("min_p", resolved.min_p, |c| c.min_p),
    ];

    for (name, value, get) in checks {
        let source = sources
            .iter()
            .find(|(field, _)| *field == name)
            .expect("field is reported")
            .1;
        match source {
            ParamSource::Layer(i) => {
                let layer = layers[i].expect("a named layer is populated");
                assert_eq!(get(layer), value, "{name}: layer {i} must carry the value");
            }
            ParamSource::Floor | ParamSource::FloorCoupled => {
                assert_eq!(get(floor), value, "{name}: must equal the floor");
            }
            ParamSource::Unset => assert_eq!(value, None, "{name}: must be absent"),
        }
    }
}

/// Across the shapes the tests above exercise individually, plus the
/// coupling-rule cases, provenance must account for every resolved value.
#[test]
fn test_sources_always_account_for_the_resolved_values() {
    let sparse_profile = InferenceConfig {
        temperature: Some(0.2),
        ..Default::default()
    };
    let recipe = InferenceConfig::reasoning_profile();
    let penalty_only = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let global = InferenceConfig {
        top_k: Some(10),
        min_p: Some(0.05),
        ..Default::default()
    };

    let ladders: [[Option<&InferenceConfig>; 4]; 6] = [
        // Nothing at all — everything falls to the floor.
        [None, None, None, None],
        // A sparse profile over a full recipe: the coupling rule fires.
        [None, Some(&sparse_profile), Some(&recipe), None],
        // The recipe alone, unclaimed from above.
        [None, None, Some(&recipe), None],
        // The drift case: a penalty above a temperature claim below it.
        [Some(&penalty_only), None, Some(&recipe), None],
        // No layer names a temperature — the trio gap-fills normally.
        [Some(&penalty_only), None, None, Some(&global)],
        // Every rung populated.
        [
            Some(&penalty_only),
            Some(&sparse_profile),
            Some(&recipe),
            Some(&global),
        ],
    ];

    for floor in [
        InferenceConfig::with_hardcoded_defaults(),
        InferenceConfig::reasoning_floor(),
    ] {
        for ladder in &ladders {
            assert_sources_explain_values(ladder, &floor);
        }
    }
}

/// `max_tokens` is the one parameter with no floor value, so an untouched
/// ladder reports it as genuinely unset rather than as a floor default.
#[test]
fn test_max_tokens_reports_unset_rather_than_floor() {
    let (_, sources) = InferenceConfig::resolve_layers_with_sources(
        &[None],
        &InferenceConfig::with_hardcoded_defaults(),
    );
    assert_eq!(sources.max_tokens, ParamSource::Unset);
    // `temperature` is now the only field with a floor to fall back on —
    // ADR 0003 deferred the other six, so they report as `Unset` for the
    // same reason `max_tokens` always has.
    assert_eq!(sources.temperature, ParamSource::Floor);
    assert_eq!(sources.top_p, ParamSource::Unset);
    assert_eq!(sources.min_p, ParamSource::Unset);
}

/// The two floor variants are distinguishable: a trio suppressed by the
/// coupling rule must not look the same as one nobody ever set.
#[test]
fn test_coupled_suppression_is_distinguishable_from_plain_absence() {
    let claim = InferenceConfig {
        temperature: Some(0.2),
        ..Default::default()
    };
    let floor = InferenceConfig::with_hardcoded_defaults();

    let (_, claimed) = InferenceConfig::resolve_layers_with_sources(&[Some(&claim)], &floor);
    assert_eq!(claimed.presence_penalty, ParamSource::FloorCoupled);

    // Since ADR 0003 the neutral floor names no `presence_penalty`, so an
    // untouched one is a genuine absence rather than a floor value. The
    // distinction the test exists for is unaffected and now sharper: the
    // coupling rule is still reported, and "nobody set this" is still a
    // different answer from "the rule discarded something".
    let (_, untouched) = InferenceConfig::resolve_layers_with_sources(&[None], &floor);
    assert_eq!(untouched.presence_penalty, ParamSource::Unset);
    assert_ne!(claimed.presence_penalty, untouched.presence_penalty);

    // And a reasoning model, whose floor *does* name it, still reports the
    // plain floor — the two floors now differ in provenance, not only in
    // value.
    let (_, reasoning) =
        InferenceConfig::resolve_layers_with_sources(&[None], &InferenceConfig::reasoning_floor());
    assert_eq!(reasoning.presence_penalty, ParamSource::Floor);
}

/// `resolve_with_profile` delegates to the explained form, so the two must
/// agree on the value, and the ladder indices must match `SamplingLayer`.
#[test]
fn test_resolve_with_profile_explained_matches_the_plain_form() {
    let profile = InferenceConfig {
        temperature: Some(0.2),
        ..Default::default()
    };
    let model = InferenceConfig::reasoning_profile();
    let ctx = ModelSamplingContext {
        is_reasoning: true,
        defaults_origin: Some(DefaultsOrigin::User),
    };

    let plain =
        InferenceConfig::default().resolve_with_profile(Some(&profile), Some(&model), None, ctx);
    let (explained, sources) = InferenceConfig::default().resolve_with_profile_explained(
        Some(&profile),
        Some(&model),
        None,
        ctx,
    );

    assert_eq!(plain, explained);
    // The profile sits at rung 1, and a user-set model at rung 2.
    assert_eq!(sources.temperature, ParamSource::Layer(1));
    assert_eq!(
        crate::domain::SamplingLayer::from_index(1),
        Some(crate::domain::SamplingLayer::Profile)
    );
    assert_eq!(sources.top_k, ParamSource::Layer(2));
    assert_eq!(
        crate::domain::SamplingLayer::from_index(2),
        Some(crate::domain::SamplingLayer::ModelUserSet)
    );
}

/// An auto-detected recipe drops to rung 4, below global settings — the
/// #685 ranking, now visible in the provenance rather than only in values.
#[test]
fn test_an_auto_detected_recipe_reports_the_lower_rung() {
    let model = InferenceConfig::reasoning_profile();
    let global = InferenceConfig {
        top_k: Some(10),
        ..Default::default()
    };
    let ctx = ModelSamplingContext {
        is_reasoning: true,
        defaults_origin: Some(DefaultsOrigin::AutoDetected),
    };

    let (_, sources) = InferenceConfig::default().resolve_with_profile_explained(
        None,
        Some(&model),
        Some(&global),
        ctx,
    );

    assert_eq!(sources.top_k, ParamSource::Layer(3), "global wins top_k");
    assert_eq!(
        sources.temperature,
        ParamSource::Layer(4),
        "the auto-detected recipe sits below global"
    );
}

#[test]
fn test_openai_json_roundtrip() {
    let config = InferenceConfig {
        temperature: Some(0.7),
        top_p: Some(0.9),
        repeat_penalty: Some(1.1),
        ..Default::default()
    };
    let patch = config.to_openai_json_patch();

    // snake_case keys present for Some fields
    assert!(patch.contains_key("temperature"));
    assert!(patch.contains_key("top_p"));
    assert!(patch.contains_key("repeat_penalty"));
    // None fields absent
    assert!(!patch.contains_key("top_k"));
    assert!(!patch.contains_key("max_tokens"));

    // Roundtrip via extract_client_sampling
    let val = serde_json::Value::Object(patch);
    let (back, _) = InferenceConfig::extract_client_sampling(&val);
    assert_eq!(back.temperature, Some(0.7));
    assert_eq!(back.top_p, Some(0.9));
    assert_eq!(back.repeat_penalty, Some(1.1));
    assert!(back.top_k.is_none());
}

#[test]
fn test_client_sampling_unknown_fields_ignored() {
    let val = serde_json::json!({
        "temperature": 0.5,
        "model": "llama3",
        "messages": []
    });
    let (config, _) = InferenceConfig::extract_client_sampling(&val);
    assert_eq!(config.temperature, Some(0.5));
    assert!(config.top_p.is_none());
}

// ── Client sampling extraction ────────────────────────────────────────

/// **The defect this was written for.** The old implementation
/// camel-cased the whole body, deserialised it as one struct and called
/// `.unwrap_or_default()`, so a single unreadable key returned an
/// all-`None` config and the client's other ten values vanished with it.
#[test]
fn one_unreadable_field_does_not_cost_the_other_ten() {
    let val = serde_json::json!({
        "temperature": 0.2,
        "top_p": 0.9,
        "top_k": 30,
        "max_tokens": "not a number",   // the offender
        "repeat_penalty": 1.1,
        "presence_penalty": 0.3,
        "min_p": 0.02,
        "dry_multiplier": 0.8,
        "dry_base": 1.75,
        "dry_allowed_length": 2,
        "dry_penalty_last_n": 64,
    });

    let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);

    assert_eq!(cfg.max_tokens, None, "the bad field is dropped");
    assert_eq!(
        issues.len(),
        1,
        "and only that field is reported: {issues:?}"
    );

    assert_eq!(cfg.temperature, Some(0.2));
    assert_eq!(cfg.top_p, Some(0.9));
    assert_eq!(cfg.top_k, Some(30));
    assert_eq!(cfg.repeat_penalty, Some(1.1));
    assert_eq!(cfg.presence_penalty, Some(0.3));
    assert_eq!(cfg.min_p, Some(0.02));
    assert_eq!(cfg.dry_multiplier, Some(0.8));
    assert_eq!(cfg.dry_base, Some(1.75));
    assert_eq!(cfg.dry_allowed_length, Some(2));
    assert_eq!(cfg.dry_penalty_last_n, Some(64));
}

/// **A client-reachable panic.** The rejected-field log line renders the
/// offending value, and it used to do so with `&s[..40]`. `serde_json`
/// does not escape non-ASCII and nothing type-checks `temperature` before
/// the pipeline, so any client could take the request task down with a
/// long enough Greek string. Asserted here rather than only in
/// `utils::text` because this is the path that actually panicked.
#[test]
fn a_multibyte_client_value_is_reported_rather_than_panicking() {
    let val = serde_json::json!({ "temperature": "α".repeat(60), "top_p": 0.9 });

    let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);

    assert_eq!(cfg.temperature, None, "the bad field is dropped");
    assert_eq!(cfg.top_p, Some(0.9), "and the rest still lands");
    assert!(
        matches!(issues.as_slice(), [FieldIssue::Rejected { field, .. }] if *field == "temperature"),
        "{issues:?}"
    );
    // The rendered value must be valid UTF-8 and marked as truncated.
    let rendered = issues[0].to_string();
    assert!(rendered.contains('…'), "{rendered}");
}

/// llama.cpp answers 200 to this, so gglib accepting it is the whole
/// point — before, it was the trip case that discarded the layer.
/// ADR 0003 finding 6.
#[test]
fn max_tokens_minus_one_means_no_limit() {
    let val = serde_json::json!({ "max_tokens": -1, "temperature": 0.4 });
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);

    assert_eq!(cfg.max_tokens, None, "-1 is the wire spelling of unset");
    assert_eq!(cfg.temperature, Some(0.4), "and the rest still lands");
    assert!(
        matches!(issues.as_slice(), [FieldIssue::Normalised { field, .. }] if *field == "max_tokens"),
        "reported as normalised, not rejected: {issues:?}"
    );
}

/// Some clients emit every number as a float. llama.cpp takes it.
#[test]
fn an_integer_valued_float_is_accepted_for_an_integer_field() {
    let val = serde_json::json!({ "top_k": 40.0 });
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
    assert_eq!(cfg.top_k, Some(40));
    assert!(matches!(issues.as_slice(), [FieldIssue::Normalised { .. }]));
}

/// A float that would lose information is not the same case.
#[test]
fn a_fractional_float_is_rejected_for_an_integer_field() {
    let val = serde_json::json!({ "top_k": 40.5 });
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
    assert_eq!(cfg.top_k, None);
    assert!(matches!(issues.as_slice(), [FieldIssue::Rejected { .. }]));
}

/// llama.cpp answers 400 to this, so gglib rejects it too rather than
/// quietly parsing a client bug into a working request.
#[test]
fn a_numeric_string_is_rejected_not_coerced() {
    let val = serde_json::json!({ "temperature": "0.7", "top_p": 0.9 });
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
    assert_eq!(cfg.temperature, None);
    assert_eq!(cfg.top_p, Some(0.9), "one bad field, one casualty");
    assert!(
        matches!(issues.as_slice(), [FieldIssue::Rejected { field, .. }] if *field == "temperature")
    );
}

/// An explicit `null` is a client saying nothing, not a client erring —
/// several of them send it for every parameter they leave at default.
#[test]
fn an_explicit_null_is_silence_rather_than_an_issue() {
    let val = serde_json::json!({ "temperature": null, "top_k": null });
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&val);
    assert_eq!(cfg.temperature, None);
    assert_eq!(cfg.top_k, None);
    assert!(issues.is_empty(), "no issue reported: {issues:?}");
}

/// The two halves have to agree, or a value gglib emits is a value gglib
/// cannot read back — which is how a round-trip through the pipeline
/// would quietly lose a field.
#[test]
fn to_patch_then_extract_is_the_identity() {
    let original = InferenceConfig {
        temperature: Some(0.35),
        top_p: Some(0.9),
        top_k: Some(30),
        max_tokens: Some(2048),
        repeat_penalty: Some(1.05),
        presence_penalty: Some(1.5),
        min_p: Some(0.05),
        dry_multiplier: Some(0.8),
        dry_base: Some(1.75),
        dry_allowed_length: Some(2),
        dry_penalty_last_n: Some(64),
        dynatemp_range: Some(0.5),
        dynatemp_exponent: Some(1.0),
        top_n_sigma: Some(1.0),
        frequency_penalty: Some(0.4),
        seed: Some(100),
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(4096),
    };

    let patch = serde_json::Value::Object(original.to_openai_json_patch());
    let (back, issues) = InferenceConfig::extract_client_sampling(&patch);

    assert_eq!(back, original);
    assert!(issues.is_empty(), "clean round trip: {issues:?}");
}

// =========================================================================
// reasoning controls (ADR 0007)
// =========================================================================

/// The same rule as [`no_floor_names_a_seed`], for a different reason.
///
/// A seed must not be floored because a floored seed makes every untuned
/// request decode identically. A reasoning control must not be floored
/// because **the template already has an answer**: `gpt-oss`'s own Jinja sets
/// `reasoning_effort = "medium"` when no kwarg arrives, and other templates
/// have other defaults or ignore the variable entirely. A floor here would
/// displace each template's own choice with one nobody made — the #739 shape,
/// on a field no readback can ever catch it doing.
///
/// The budget is the same argument in integers: `-1` already means "defer to
/// the launch `--reasoning-budget`", which is exactly what emitting no key
/// does.
#[test]
fn no_floor_names_a_reasoning_control() {
    for floor in [
        InferenceConfig::with_hardcoded_defaults(),
        InferenceConfig::reasoning_floor(),
        InferenceConfig::reasoning_profile(),
    ] {
        assert_eq!(floor.reasoning_effort, None);
        assert_eq!(floor.reasoning_budget_tokens, None);
    }
}

/// Unfloored has to mean *no key on the wire*, not a `null`. A `null`
/// `reasoning_effort` is a non-string, which llama-server silently degrades to
/// the template's own default — the same outcome by accident, and untraceable.
#[test]
fn an_unset_reasoning_control_emits_no_key() {
    let patch = InferenceConfig::with_hardcoded_defaults().to_openai_json_patch();
    assert!(!patch.contains_key("reasoning_effort"));
    assert!(!patch.contains_key("reasoning_budget_tokens"));
}

/// **The emission pin.** `to_openai_json_patch` works by serde reflection plus
/// a camelCase→`snake_case` rename, so nothing hand-writes these keys and
/// nothing would fail if the rename produced `reasoning_effort_tokens` or the
/// enum serialised as `"High"`. llama-server validates neither field, so a
/// wrong key or a wrong casing would be accepted, ignored, and reported
/// nowhere.
#[test]
fn the_reasoning_controls_reach_the_wire_under_their_openai_names() {
    let patch = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(4096),
        ..InferenceConfig::default()
    }
    .to_openai_json_patch();

    assert_eq!(
        patch.get("reasoning_effort"),
        Some(&serde_json::json!("high"))
    );
    assert_eq!(
        patch.get("reasoning_budget_tokens"),
        Some(&serde_json::json!(4096))
    );
}

/// `xhigh` end to end, because it is the one level whose wire spelling a
/// plausible serde attribute (`snake_case`) would get wrong — and getting it
/// wrong would render `Reasoning: x_high` into a prompt rather than failing.
#[test]
fn xhigh_survives_the_patch_as_one_word() {
    let patch = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::XHigh),
        ..InferenceConfig::default()
    }
    .to_openai_json_patch();

    assert_eq!(
        patch.get("reasoning_effort"),
        Some(&serde_json::json!("xhigh"))
    );
}

/// `0` is a real value — "stop thinking immediately" — and the whole reason
/// gglib can refuse to offer `reasoning_effort: "none"`. It must not be
/// mistaken for an absence anywhere between here and the wire.
#[test]
fn a_zero_reasoning_budget_is_a_value_and_not_an_absence() {
    let patch = InferenceConfig {
        reasoning_budget_tokens: Some(0),
        ..InferenceConfig::default()
    }
    .to_openai_json_patch();

    assert_eq!(
        patch.get("reasoning_budget_tokens"),
        Some(&serde_json::json!(0))
    );
}

#[test]
fn a_client_effort_level_is_read_in_any_case() {
    for (sent, expected) in [
        ("minimal", ReasoningEffort::Minimal),
        ("low", ReasoningEffort::Low),
        ("medium", ReasoningEffort::Medium),
        ("high", ReasoningEffort::High),
        ("xhigh", ReasoningEffort::XHigh),
        ("max", ReasoningEffort::Max),
        ("HIGH", ReasoningEffort::High),
    ] {
        let (cfg, issues) = InferenceConfig::extract_client_sampling(
            &serde_json::json!({ "reasoning_effort": sent }),
        );
        assert_eq!(cfg.reasoning_effort, Some(expected), "{sent}");
        assert!(issues.is_empty(), "{sent}: {issues:?}");
    }
}

/// The measured wire fact this enum exists for: llama-server accepts
/// `"banana"` and renders it into the prompt. gglib rejects it *by name*, so
/// the client learns and the other ten fields it sent are untouched.
#[test]
fn an_unknown_effort_level_is_rejected_and_costs_only_itself() {
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&serde_json::json!({
        "reasoning_effort": "banana",
        "temperature": 0.42,
    }));

    assert_eq!(cfg.reasoning_effort, None);
    assert_eq!(cfg.temperature, Some(0.42));
    assert!(
        matches!(
            issues.as_slice(),
            [FieldIssue::Rejected { field, .. }] if *field == "reasoning_effort"
        ),
        "{issues:?}"
    );
}

/// A non-string does not fail upstream either — it degrades to the template's
/// own default, silently. Rejecting it is the only way anyone finds out.
#[test]
fn a_non_string_effort_level_is_rejected_rather_than_coerced() {
    for sent in [
        serde_json::json!(42),
        serde_json::json!(true),
        serde_json::json!(["high"]),
        serde_json::json!({"a": 1}),
    ] {
        let (cfg, issues) = InferenceConfig::extract_client_sampling(
            &serde_json::json!({ "reasoning_effort": sent }),
        );
        assert_eq!(cfg.reasoning_effort, None, "{sent}");
        assert!(
            matches!(issues.as_slice(), [FieldIssue::Rejected { .. }]),
            "{sent}: {issues:?}"
        );
    }
}

/// llama-server ignores an empty string, and so does this type — but it says
/// so. A client sending `""` on every request is a fact worth being able to
/// see, and `Normalised` is how the other readers already say "taken to mean
/// nothing".
#[test]
fn an_empty_effort_level_normalises_to_no_opinion() {
    let (cfg, issues) =
        InferenceConfig::extract_client_sampling(&serde_json::json!({"reasoning_effort": ""}));

    assert_eq!(cfg.reasoning_effort, None);
    assert!(
        matches!(
            issues.as_slice(),
            [FieldIssue::Normalised { field, .. }] if *field == "reasoning_effort"
        ),
        "{issues:?}"
    );
}

/// ADR 0007 decision 4. `"none"` is the one wrong value a client is likely to
/// send deliberately, so the rejection points at the field that actually works
/// instead of guessing at an intent only the client knows.
#[test]
fn none_is_rejected_and_the_message_names_the_budget() {
    let (cfg, issues) =
        InferenceConfig::extract_client_sampling(&serde_json::json!({"reasoning_effort": "none"}));

    assert_eq!(cfg.reasoning_effort, None);
    let [FieldIssue::Rejected { expected, .. }] = issues.as_slice() else {
        panic!("expected one rejection, got {issues:?}");
    };
    assert!(
        expected.contains("reasoning_budget_tokens: 0"),
        "the rejection must point somewhere useful: {expected}"
    );
}

/// The budget accepts exactly what upstream accepts — no narrower. `-1` defers
/// to the launch default, `0` stops thinking, and `i32::MAX` is the top of the
/// range llama-server's own 400 names.
#[test]
fn the_budget_accepts_upstreams_whole_range() {
    for sent in [-1, 0, 1, 4096, i32::MAX] {
        let (cfg, issues) = InferenceConfig::extract_client_sampling(
            &serde_json::json!({ "reasoning_budget_tokens": sent }),
        );
        assert_eq!(cfg.reasoning_budget_tokens, Some(sent), "{sent}");
        assert!(issues.is_empty(), "{sent}: {issues:?}");
    }
}

/// And rejects exactly what upstream rejects. `-2` is the value the live probe
/// measured coming back as an HTTP 400 naming the range; gglib reproduces that
/// verdict rather than inventing one, which is the whole difference between
/// this field and its twin.
#[test]
fn a_budget_below_minus_one_is_rejected_the_way_upstream_rejects_it() {
    let (cfg, issues) = InferenceConfig::extract_client_sampling(
        &serde_json::json!({"reasoning_budget_tokens": -2}),
    );

    assert_eq!(cfg.reasoning_budget_tokens, None);
    assert!(
        matches!(
            issues.as_slice(),
            [FieldIssue::Rejected { field, .. }] if *field == "reasoning_budget_tokens"
        ),
        "{issues:?}"
    );
}

/// Upstream reads two names for one parameter, so the reader has to.
///
/// A name gglib does not read is a name the trust gate cannot govern: before
/// this, `thinking_budget_tokens` entered no layer, joined no discard record
/// and was overwritten by no force-insert, so it reached llama-server intact
/// whatever the operator had resolved — and since neither reasoning control is
/// observable afterwards (ADR 0007 finding 7a), nothing would ever have said
/// so. The alias is accepted over exactly the same range as the canonical key.
#[test]
fn the_budget_is_read_under_upstreams_alias_too() {
    for sent in [-1, 0, 4096] {
        let (cfg, issues) = InferenceConfig::extract_client_sampling(
            &serde_json::json!({ "thinking_budget_tokens": sent }),
        );
        assert_eq!(cfg.reasoning_budget_tokens, Some(sent), "{sent}");
        assert!(issues.is_empty(), "{sent}: {issues:?}");
    }
}

/// With both names present the canonical one wins.
///
/// It is the name gglib itself emits, the name the provenance record and the
/// audit report, and the name every operator-facing surface prints — so a
/// request where the two disagree must resolve to the one everything else in
/// the system will call it. `null` is an absence under either name, exactly as
/// it is for every other field here, so an explicitly-nulled canonical key
/// leaves the alias to speak.
#[test]
fn the_canonical_budget_key_wins_over_the_alias() {
    let (cfg, issues) = InferenceConfig::extract_client_sampling(&serde_json::json!({
        "reasoning_budget_tokens": 256,
        "thinking_budget_tokens": 100_000,
    }));
    assert_eq!(cfg.reasoning_budget_tokens, Some(256));
    assert!(issues.is_empty(), "{issues:?}");

    let (nulled, issues) = InferenceConfig::extract_client_sampling(&serde_json::json!({
        "reasoning_budget_tokens": serde_json::Value::Null,
        "thinking_budget_tokens": 512,
    }));
    assert_eq!(nulled.reasoning_budget_tokens, Some(512));
    assert!(issues.is_empty(), "{issues:?}");
}

/// A refusal names the key the client actually sent.
///
/// `client_fields_rejected` is what an operator reads when a request did not do
/// what its author expected, and naming a canonical key the client never typed
/// would send them looking for a field that is not in their request. It is also
/// the key the body cleanup removes, so the two must agree.
#[test]
fn an_aliased_budget_is_rejected_under_the_name_it_arrived_with() {
    let (cfg, issues) = InferenceConfig::extract_client_sampling(
        &serde_json::json!({"thinking_budget_tokens": -2}),
    );

    assert_eq!(cfg.reasoning_budget_tokens, None);
    assert!(
        matches!(
            issues.as_slice(),
            [FieldIssue::Rejected { field, .. }] if *field == "thinking_budget_tokens"
        ),
        "{issues:?}"
    );
}

/// Neither control is coupled to `temperature`, and this is the failure that
/// would prove it if they were: a profile naming only an effort level would
/// claim the coupled trio and strip the model's tuned `presence_penalty`.
///
/// They are uncoupled because they cannot interact with the distribution the
/// trio shapes — one is a template kwarg consumed before sampling starts, the
/// other is a token count.
#[test]
fn a_profile_naming_only_an_effort_does_not_strip_the_models_recipe() {
    let profile = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        ..InferenceConfig::default()
    };
    let model = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        min_p: Some(0.0),
        ..InferenceConfig::default()
    };

    let resolved = InferenceConfig::default().resolve_with_profile(
        Some(&profile),
        Some(&model),
        None,
        ModelSamplingContext {
            is_reasoning: true,
            ..ModelSamplingContext::default()
        },
    );

    assert_eq!(resolved.reasoning_effort, Some(ReasoningEffort::High));
    assert_eq!(resolved.temperature, Some(1.0));
    assert_eq!(resolved.presence_penalty, Some(1.5));
    assert_eq!(resolved.min_p, Some(0.0));
}

/// The other direction: a layer claiming the temperature must not drag the
/// reasoning controls out of the layers beneath it either.
#[test]
fn a_claimed_temperature_leaves_the_reasoning_controls_gap_filling() {
    let profile = InferenceConfig {
        temperature: Some(0.2),
        ..InferenceConfig::default()
    };
    let model = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        reasoning_effort: Some(ReasoningEffort::Max),
        reasoning_budget_tokens: Some(2048),
        ..InferenceConfig::default()
    };

    let resolved = InferenceConfig::default().resolve_with_profile(
        Some(&profile),
        Some(&model),
        None,
        ModelSamplingContext::default(),
    );

    assert_eq!(resolved.temperature, Some(0.2));
    // The trio was claimed by the profile, which named none of it, so it
    // dropped to a floor that asserts none of it either.
    assert_eq!(resolved.presence_penalty, None);
    // The reasoning controls were not claimed, because they are not part of
    // the set — they gap-fill from the model like any uncoupled parameter.
    assert_eq!(resolved.reasoning_effort, Some(ReasoningEffort::Max));
    assert_eq!(resolved.reasoning_budget_tokens, Some(2048));
}

/// `merge_with` is `const`, and stays that way only while every field it moves
/// is `Copy`. A `String` effort level would have forced the keyword off it —
/// quietly, since dropping `const` breaks no caller and turns every merge into
/// a clone.
///
/// A `const fn` may only call other `const fn`s, so this wrapper fails to
/// compile the moment `merge_with` stops being one. It is a compile-time
/// assertion wearing a test's clothes; the `#[test]` exists so `cargo test`
/// reports the name.
#[test]
fn merge_with_is_still_a_const_fn() {
    const fn assert_const(base: &mut InferenceConfig, fallback: &InferenceConfig) {
        base.merge_with(fallback);
    }

    let mut base = InferenceConfig {
        reasoning_budget_tokens: Some(1),
        ..InferenceConfig::default()
    };
    let fallback = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::Low),
        reasoning_budget_tokens: Some(999),
        ..InferenceConfig::default()
    };
    assert_const(&mut base, &fallback);

    assert_eq!(base.reasoning_effort, Some(ReasoningEffort::Low));
    assert_eq!(base.reasoning_budget_tokens, Some(1));
}
