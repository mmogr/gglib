//! Tests for [`super::resolve_sampling`] and the sampling layer cascade.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;
use crate::domain::ParamSource;
use serde_json::json;

fn temp(value: f32) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(value),
        ..Default::default()
    }
}

fn model_ctx(defaults: Option<InferenceConfig>) -> ModelContext {
    ModelContext {
        inference_defaults: defaults,
        ..ModelContext::passthrough()
    }
}

/// `f32 → f64` widening makes exact literal comparison unreliable.
#[track_caller]
fn assert_param(body: &Value, key: &str, expected: f64) {
    let actual = body
        .get(key)
        .and_then(Value::as_f64)
        .unwrap_or_else(|| panic!("{key} missing from body: {body}"));
    assert!(
        (actual - expected).abs() < 1e-6,
        "{key}: expected {expected}, got {actual}"
    );
}

/// The parameter reached the wire as an *absence*, so llama.cpp's own
/// default applies.
///
/// The normal outcome for six of the seven since ADR 0003. Asserting the
/// key is missing is the whole point — a value here, even the right one,
/// means gglib is overriding whatever upstream chooses next.
#[track_caller]
fn assert_deferred(body: &Value, key: &str) {
    assert!(
        body.get(key).is_none(),
        "{key} must be deferred to llama.cpp, but the body carries {body}"
    );
}

// ── The hierarchy ─────────────────────────────────────────────────────

/// One table, one row per layer: each wins only over the ones beneath it.
#[test]
fn each_layer_beats_the_ones_below_it() {
    let cases = [
        // (cli, client temperature, profile, model, global, expected, why)
        (
            Some(0.05),
            Some(0.11),
            Some(0.22),
            Some(0.33),
            Some(0.44),
            0.05,
            "cli override beats client",
        ),
        (
            None,
            Some(0.11),
            Some(0.22),
            Some(0.33),
            Some(0.44),
            0.11,
            "client beats profile",
        ),
        (
            None,
            None,
            Some(0.22),
            Some(0.33),
            Some(0.44),
            0.22,
            "profile beats model",
        ),
        (
            None,
            None,
            None,
            Some(0.33),
            Some(0.44),
            0.33,
            "model beats global",
        ),
        (
            None,
            None,
            None,
            None,
            Some(0.44),
            0.44,
            "global beats hardcoded",
        ),
        (None, None, None, None, None, 0.7, "hardcoded fallback"),
    ];

    for (cli, client, profile, model, global, expected, why) in cases {
        let mut body = client.map_or_else(|| json!({}), |t| json!({"temperature": t}));
        let layers = SamplingLayers {
            cli_override: cli.map(temp),
            profile: profile.map(temp),
            global: global.map(temp),
            // This table is specifically about layer precedence, not
            // about the trust gate — trust it here so the "client beats
            // profile" / "cli beats client" rows still exercise the
            // client layer at all. See `client_sampling_is_ignored_by_default`.
            trust_client_sampling: true,
            agentic_adjustments: false,
        };
        resolve_sampling(&mut body, &model_ctx(model.map(temp)), &layers);
        assert_param(&body, "temperature", expected);
        assert!(
            body["temperature"].as_f64().is_some(),
            "{why}: temperature must be present"
        );
    }
}

/// Profiles are sparse: outranking the model layer must not blank out the
/// untuned parameters the profile says nothing about.
#[test]
fn a_sparse_profile_leaves_other_model_defaults_intact() {
    let mut body = json!({});
    let model = InferenceConfig {
        temperature: Some(1.0),
        top_p: Some(0.87),
        top_k: Some(20),
        ..Default::default()
    };
    resolve_sampling(
        &mut body,
        &model_ctx(Some(model)),
        &SamplingLayers {
            cli_override: None,
            profile: Some(temp(0.2)),
            global: None,
            trust_client_sampling: false,
            agentic_adjustments: false,
        },
    );

    assert_param(&body, "temperature", 0.2);
    assert_param(&body, "top_p", 0.87);
    assert_param(&body, "top_k", 20.0);
}

// ── The returned decision ─────────────────────────────────────────────

/// The pipeline's own provenance, end to end.
///
/// Every other provenance test in this file rebuilds a ladder by hand and
/// calls `resolve_layers_with_sources` directly, so none of them touches
/// the ladder `resolve_sampling` actually builds. That is how three doc
/// comments drifted to three different rung counts unnoticed.
#[test]
fn the_returned_sources_describe_the_ladder_the_pipeline_built() {
    let mut body = json!({ "messages": [], "temperature": 0.25 });
    let decision = resolve_sampling(
        &mut body,
        &model_ctx(None),
        &SamplingLayers {
            trust_client_sampling: true,
            ..Default::default()
        },
    );

    assert_eq!(decision.layer_names.len(), LADDER_RUNGS);
    assert_eq!(decision.layer_names[0], "cli");
    assert_eq!(decision.layer_names[1], "client");
    assert_eq!(
        decision.layer_names[LADDER_RUNGS - 1],
        "model (auto-detected)"
    );

    // The client rung is index 1, and the client is what named it.
    assert_eq!(decision.sources.temperature, ParamSource::Layer(1));
    assert_eq!(decision.resolved.temperature, Some(0.25));
    assert!(decision.applied);
    assert_eq!(decision.floor, FloorClass::Default);

    // And the decision agrees with what actually reached the body.
    assert_param(&body, "temperature", 0.25);
}

/// A body that is not an object resolves but does not apply. A readback
/// needs the two distinguishable: nothing was sent, so nothing can
/// diverge.
#[test]
fn a_non_object_body_reports_resolved_but_not_applied() {
    let mut body = json!("not an object");
    let decision = resolve_sampling(&mut body, &model_ctx(None), &SamplingLayers::default());

    assert!(!decision.applied);
    assert!(decision.resolved.temperature.is_some(), "still resolved");
}

/// The interaction six existing tests could only assert by value.
///
/// When the ceiling bites, `sources.temperature` must still name the rung
/// that supplied the value — the cap does not replace that rung, it caps
/// what the rung supplied. Reporting `floor` here would make the log say
/// nobody chose a temperature on a model whose recipe did.
///
/// A non-reasoning model, because that is the only class that still has a
/// ceiling — see `a_reasoning_models_recipe_stands_uncapped`.
#[test]
fn the_ceiling_caps_the_value_without_rewriting_its_provenance() {
    let mut body = json!({
        "messages": [],
        "tools": [{ "type": "function", "function": { "name": "f" } }],
    });
    let ctx = ModelContext {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(1.0),
            ..Default::default()
        }),
        defaults_origin: Some(DefaultsOrigin::AutoDetected),
        ..ModelContext::passthrough()
    };
    let decision = resolve_sampling(
        &mut body,
        &ctx,
        &SamplingLayers {
            agentic_adjustments: true,
            ..Default::default()
        },
    );

    assert!(decision.agentic_turn);
    assert_eq!(decision.agentic_ceiling_applied, Some(0.3));
    assert_eq!(decision.resolved.temperature, Some(0.3), "capped");
    assert_eq!(
        decision.sources.temperature,
        ParamSource::Layer(LADDER_RUNGS - 1),
        "provenance still names the auto-detected rung that supplied 1.0"
    );
    assert_eq!(decision.floor, FloorClass::Default);
}

/// The trust gate's discard is the largest silent drop gglib performs, and
/// it is the default posture. It has to be nameable.
#[test]
fn the_trust_gate_reports_what_it_dropped() {
    let mut body = json!({
        "messages": [],
        "temperature": 0.9,
        "top_p": 0.5,
        "max_tokens": 128,
    });
    let decision = resolve_sampling(&mut body, &model_ctx(None), &SamplingLayers::default());

    let dropped = &decision.client_fields_discarded;
    assert!(dropped.contains(&"temperature".to_string()), "{dropped:?}");
    assert!(dropped.contains(&"top_p".to_string()), "{dropped:?}");
    assert!(
        !dropped.contains(&"max_tokens".to_string()),
        "max_tokens survives the gate by design: {dropped:?}"
    );
}

/// An unreadable client field is carried out rather than swallowed.
#[test]
fn an_unreadable_client_field_is_reported_on_the_decision() {
    let mut body = json!({ "messages": [], "temperature": "0.7" });
    let decision = resolve_sampling(
        &mut body,
        &model_ctx(None),
        &SamplingLayers {
            trust_client_sampling: true,
            ..Default::default()
        },
    );

    assert_eq!(decision.client_fields_rejected.len(), 1);
}

// ── Provenance ────────────────────────────────────────────────────────

/// The six names the pipeline's own ladder uses, for the provenance
/// tests below.
///
/// Six, not five. This helper was 5-wide while `resolve_sampling` built a
/// 6-rung ladder, so nothing exercised the real index→name mapping and
/// three separate doc comments drifted to three different rung counts
/// before anyone noticed. Keep it the same width as the array at the top
/// of `resolve_sampling` or it stops testing the thing it looks like it
/// tests.
const LAYER_NAMES: [&str; 6] = [
    "cli",
    "client",
    "profile",
    "model",
    "global",
    "model (auto-detected)",
];

/// Resolve a ladder and render its provenance the way the debug line does.
fn provenance_of(layers: &[Option<&InferenceConfig>; 6]) -> String {
    let floor = InferenceConfig::with_hardcoded_defaults();
    InferenceConfig::resolve_layers_with_sources(layers, &floor)
        .1
        .describe(&LAYER_NAMES)
}

/// The `:coding` shape. The provenance must say the penalty came from the
/// floor, not from the model — otherwise the log would assert exactly the
/// leak the merge now prevents.
#[test]
fn provenance_reports_coupling_suppressed_layers_as_floor() {
    let model = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        top_k: Some(20),
        ..Default::default()
    };
    let profile = temp(0.2);
    let got = provenance_of(&[None, None, Some(&profile), Some(&model), None, None]);

    assert!(got.contains("temperature=profile"), "{got}");
    assert!(got.contains("presence_penalty=floor"), "{got}");
    // Untuned parameters are unaffected by the claim.
    assert!(got.contains("top_k=model"), "{got}");
}

/// With nothing above it claiming a temperature, the model's own recipe is
/// reported intact.
#[test]
fn provenance_attributes_an_unclaimed_recipe_to_the_model() {
    let model = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        ..Default::default()
    };
    let got = provenance_of(&[None, None, None, Some(&model), None, None]);

    assert!(got.contains("temperature=model"), "{got}");
    assert!(got.contains("presence_penalty=model"), "{got}");
}

/// Operator flags are reported as their own layer, above the client.
#[test]
fn provenance_names_the_cli_layer() {
    let cli = temp(0.3);
    let client = temp(0.9);
    let got = provenance_of(&[Some(&cli), Some(&client), None, None, None, None]);

    assert!(got.contains("temperature=cli"), "{got}");
}

/// The drift this unification removes. `cli` names a `presence_penalty`
/// but no `temperature`; `model` claims the temperature, so the coupling
/// rule resolves the penalty from `model` — and the provenance must say so.
///
/// The previous `describe_provenance` scanned every layer down to the
/// claiming one and reported `cli`, naming a layer the resolution had
/// passed over.
#[test]
fn provenance_does_not_credit_a_layer_the_coupling_rule_passed_over() {
    let cli = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let model = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        ..Default::default()
    };
    let layers = [Some(&cli), None, None, Some(&model), None, None];

    let got = provenance_of(&layers);
    assert!(
        got.contains("presence_penalty=model"),
        "the claiming layer supplied it, got: {got}"
    );

    // And the value agrees with the name.
    let floor = InferenceConfig::with_hardcoded_defaults();
    let (resolved, _) = InferenceConfig::resolve_layers_with_sources(&layers, &floor);
    assert_eq!(resolved.presence_penalty, Some(1.5));
}

/// Regression for #621: operator flags must beat the per-model layer.
///
/// These previously merged into the *global* layer, which sits below the
/// model — so on any model with stored `inference_defaults`, every
/// `gglib proxy --temperature …` style flag silently did nothing.
#[test]
fn a_cli_override_beats_the_model_layer() {
    let mut body = json!({});
    resolve_sampling(
        &mut body,
        &model_ctx(Some(InferenceConfig {
            temperature: Some(1.0),
            top_k: Some(20),
            ..Default::default()
        })),
        &SamplingLayers {
            cli_override: Some(temp(0.3)),
            ..Default::default()
        },
    );

    assert_param(&body, "temperature", 0.3);
    // Untuned parameters the operator said nothing about still resolve.
    assert_param(&body, "top_k", 20.0);
}

/// The operator runs the server, so their flags also outrank the client's
/// own request parameters — otherwise any caller could quietly ignore them.
#[test]
fn a_cli_override_beats_client_request_params() {
    let mut body = json!({"temperature": 0.9});
    resolve_sampling(
        &mut body,
        &model_ctx(None),
        &SamplingLayers {
            cli_override: Some(temp(0.3)),
            ..Default::default()
        },
    );

    assert_param(&body, "temperature", 0.3);
}

/// Regression for #621, at the pipeline level: the `:coding` shape — a
/// profile that lowers the temperature — must not carry the model's
/// `presence_penalty`, which was tuned for the model's own temperature.
#[test]
fn a_profile_temperature_does_not_carry_model_penalties() {
    let mut body = json!({});
    let model = InferenceConfig {
        temperature: Some(1.0),
        presence_penalty: Some(1.5),
        ..Default::default()
    };
    resolve_sampling(
        &mut body,
        &model_ctx(Some(model)),
        &SamplingLayers {
            cli_override: None,
            profile: Some(temp(0.2)),
            global: None,
            trust_client_sampling: false,
            agentic_adjustments: false,
        },
    );

    assert_param(&body, "temperature", 0.2);
    // The profile claimed the temperature and named no penalty, so the
    // model's 1.5 is passed over. Nothing is sent: the floor used to
    // restate upstream's 0.0 here, and ADR 0003 deferred it, so the model
    // still decodes at 0.0 — supplied by llama.cpp rather than by gglib.
    assert_deferred(&body, "presence_penalty");
}

/// When the client IS trusted (`trust_client_sampling: true` — an
/// `OpenWebUI`-style client with real sampling controls exposed to its
/// user), a client that sends `temperature: 0` must still not silently
/// zero out a reasoning model's only anti-repetition guard. The client
/// still wins the temperature it asked for — it just doesn't also claim
/// penalties it never named an opinion on. See `resolve_layers_with_sources`'s
/// coupling rule.
#[test]
fn trusted_client_temperature_zero_does_not_zero_a_reasoning_models_presence_penalty() {
    let mut body = json!({"temperature": 0.0});
    let ctx = ModelContext {
        tags: vec!["reasoning".to_owned()],
        inference_defaults: Some(InferenceConfig::reasoning_profile()),
        ..ModelContext::passthrough()
    };
    resolve_sampling(
        &mut body,
        &ctx,
        &SamplingLayers {
            trust_client_sampling: true,
            ..Default::default()
        },
    );

    assert_param(&body, "temperature", 0.0);
    assert_param(&body, "presence_penalty", 1.0);
}

/// Same as above, but a non-reasoning model gets the plain neutral floor,
/// not the reasoning one — the class floor is opt-in via the tag, not a
/// blanket change.
#[test]
fn trusted_client_temperature_zero_leaves_a_non_reasoning_model_at_the_neutral_floor() {
    let mut body = json!({"temperature": 0.0});
    let model = InferenceConfig {
        temperature: Some(0.8),
        presence_penalty: Some(0.6),
        ..Default::default()
    };
    resolve_sampling(
        &mut body,
        &model_ctx(Some(model)),
        &SamplingLayers {
            trust_client_sampling: true,
            ..Default::default()
        },
    );

    assert_param(&body, "temperature", 0.0);
    // The client claimed the temperature, so the model's 0.6 is passed
    // over and no penalty is asserted — a non-reasoning model gets
    // llama.cpp's neutral 0.0 rather than gglib restating it.
    assert_deferred(&body, "presence_penalty");
}

// ── Client sampling authority (Settings.trust_client_sampling) ─────────

/// The default. This is the actual fix for the incident that motivated
/// this whole refactor: without a client-trust escape hatch, a client
/// hardcoding `temperature: 0` with no way for its user to change it (VS
/// Code Copilot's LLM Gateway) claims the coupled set on every request
/// and supplies none of it — so the model's own tuned recipe never has a
/// chance to apply, no matter what `resolve_layers_with_sources`'s coupling rule does.
/// With the client out of the ladder entirely, the model's full recipe —
/// temperature *and* the penalties tuned for it — resolves untouched.
#[test]
fn client_sampling_is_ignored_by_default() {
    let mut body = json!({"temperature": 0.0});
    let ctx = ModelContext {
        tags: vec!["reasoning".to_owned()],
        inference_defaults: Some(InferenceConfig::reasoning_profile()),
        ..ModelContext::passthrough()
    };
    resolve_sampling(&mut body, &ctx, &SamplingLayers::default());

    assert_param(&body, "temperature", 1.0); // the model's own, not the client's 0.0
    assert_param(&body, "presence_penalty", 1.5); // the model's tuned recipe, intact
}

/// `max_tokens` is a budget, not a taste — it stays client-authoritative
/// even when nothing else about the client's request is trusted, because
/// dropping it would silently truncate that client's own turns.
#[test]
fn max_tokens_is_still_honoured_when_client_sampling_is_untrusted() {
    let mut body = json!({"temperature": 0.9, "max_tokens": 999});
    resolve_sampling(
        &mut body,
        &model_ctx(Some(InferenceConfig {
            temperature: Some(0.4),
            ..Default::default()
        })),
        &SamplingLayers::default(),
    );

    assert_param(&body, "temperature", 0.4); // client's 0.9 dropped
    assert_param(&body, "max_tokens", 999.0); // client's budget still honoured
}

// ── Unmodelled sampler keys (the strip beside the gate) ─────────────────

/// The gate discards the client's *layer*, but the resolved patch is only
/// inserted — so before the strip existed, a sampler key the ladder has
/// no field for rode the body straight to llama-server, past every rule
/// above. `mirostat` alone replaces the whole truncation stack.
#[test]
fn untrusted_unmodelled_sampler_keys_are_stripped_and_recorded() {
    let mut body = json!({
        "temperature": 0.9,
        "mirostat": 2,
        "typical_p": 0.5,
        "xtc_probability": 0.3,
    });
    let decision = resolve_sampling(
        &mut body,
        &model_ctx(Some(temp(0.4))),
        &SamplingLayers::default(),
    );

    assert_param(&body, "temperature", 0.4); // the gate, as before
    let obj = body.as_object().unwrap();
    assert!(!obj.contains_key("mirostat"), "{body}");
    assert!(!obj.contains_key("typical_p"), "{body}");
    assert!(!obj.contains_key("xtc_probability"), "{body}");

    // Both kinds of drop land in one record: the modelled field the gate
    // binned and the unmodelled keys the strip removed.
    for dropped in ["temperature", "mirostat", "typical_p", "xtc_probability"] {
        assert!(
            decision
                .client_fields_discarded
                .iter()
                .any(|k| k == dropped),
            "{dropped} missing from {:?}",
            decision.client_fields_discarded
        );
    }
}

/// The regression the live check caught. A gated *modelled* key whose
/// field then resolves to nothing — the normal state of every deferred
/// parameter since ADR 0003 — used to survive in the body, because
/// force-insert only overwrites keys the resolution emits. Measured on a
/// live server: an untrusted `frequency_penalty: 0.9` reached `/slots`
/// intact. The gate's drops must leave the body, not just the layer.
#[test]
fn a_gated_key_the_ladder_stays_silent_on_leaves_the_body() {
    let mut body = json!({
        "frequency_penalty": 0.9,
        "top_p": 0.3,
        "dry_base": 3.5,
        "seed": 42,
        "max_tokens": 128,
    });
    // Nothing above the floor names any of these, and the neutral floor
    // asserts none of them — resolution emits no key for them at all.
    resolve_sampling(&mut body, &model_ctx(None), &SamplingLayers::default());

    let obj = body.as_object().unwrap();
    for gone in ["frequency_penalty", "top_p", "dry_base", "seed"] {
        assert!(!obj.contains_key(gone), "{gone} survived in {body}");
    }
    // The one client field the gate honours survives untouched.
    assert_param(&body, "max_tokens", 128.0);
}

/// Trusted means trusted: a client the operator vouched for keeps its
/// unmodelled keys exactly as it keeps its modelled ones.
#[test]
fn a_trusted_clients_unmodelled_sampler_keys_survive() {
    let mut body = json!({"mirostat": 2, "mirostat_tau": 4.0});
    let decision = resolve_sampling(
        &mut body,
        &model_ctx(None),
        &SamplingLayers {
            trust_client_sampling: true,
            ..Default::default()
        },
    );

    let obj = body.as_object().unwrap();
    assert_eq!(obj.get("mirostat"), Some(&json!(2)), "{body}");
    assert_eq!(obj.get("mirostat_tau"), Some(&json!(4.0)), "{body}");
    assert!(decision.client_fields_discarded.is_empty());
}

/// The strip is scoped to taste, not function: budgets, stops, constraint
/// machinery and observation fields say what the request *is* and stay
/// client-authoritative even untrusted. `logit_bias` is the deliberate
/// edge — per-token surgery with functional uses, kept until a dedicated
/// decision says otherwise.
#[test]
fn the_strip_leaves_functional_keys_alone() {
    let mut body = json!({
        "stop": ["\n\n"],
        "logit_bias": {"1234": -100},
        "response_format": {"type": "json_object"},
        "n_probs": 5,
    });
    resolve_sampling(&mut body, &model_ctx(None), &SamplingLayers::default());

    let obj = body.as_object().unwrap();
    for key in ["stop", "logit_bias", "response_format", "n_probs"] {
        assert!(obj.contains_key(key), "{key} was stripped from {body}");
    }
}

/// A modelled key must never be in the strip list: the gate already
/// governs those, and stripping one would delete the client's value
/// before the *trusted* path could read it. This is what forces the list
/// to shrink when a parameter gets modelled, the way `frequency_penalty`
/// just was.
#[test]
fn no_modelled_key_is_listed_as_unmodelled() {
    // A full literal on purpose: a new field fails this construction and
    // forces its author to decide whether the strip list must shrink.
    let every_field = InferenceConfig {
        temperature: Some(0.7),
        top_p: Some(0.95),
        top_k: Some(40),
        max_tokens: Some(512),
        repeat_penalty: Some(1.0),
        presence_penalty: Some(0.5),
        frequency_penalty: Some(0.5),
        min_p: Some(0.05),
        dynatemp_range: Some(0.5),
        dynatemp_exponent: Some(1.0),
        top_n_sigma: Some(1.0),
        dry_multiplier: Some(0.8),
        dry_base: Some(1.75),
        dry_allowed_length: Some(2),
        dry_penalty_last_n: Some(64),
        seed: Some(100),
    };
    for key in every_field.to_openai_json_patch().keys() {
        assert!(
            !UNMODELLED_SAMPLER_KEYS.contains(&key.as_str()),
            "{key} is modelled and must not be stripped"
        );
    }
}

// ── Measured defaults (DefaultsOrigin::Measured) ────────────────────────

/// A tune sweep's winner, with the same shape and tags. Only the origin
/// differs from [`auto_detected_ctx`] — which is the point: everything a
/// measured recipe does differently hangs off that one field.
fn measured_ctx(defaults: InferenceConfig, reasoning: bool) -> ModelContext {
    ModelContext {
        defaults_origin: Some(DefaultsOrigin::Measured),
        ..auto_detected_ctx(defaults, reasoning)
    }
}

/// The ladder's oldest principle holds for measurements too: nothing a
/// person chose may be outranked by anything a person did not, and an
/// automated apply is not a person.
#[test]
fn a_measured_recipe_ranks_below_global_settings() {
    let mut body = json!({});
    resolve_sampling(
        &mut body,
        &measured_ctx(temp(0.9), false),
        &SamplingLayers {
            global: Some(temp(0.5)),
            ..Default::default()
        },
    );
    assert_param(&body, "temperature", 0.5);
}

/// The regression guard this variant exists for. The sweep resolved its
/// candidates against the model's real context (#748) so the winner
/// transfers to production — a ceiling capping the stored winner would
/// un-measure it on exactly the turns it was measured for. The
/// auto-detected contrast in the same test pins that the exemption is the
/// origin, not a loosening of the ceiling.
#[test]
fn the_agentic_ceiling_never_caps_a_measured_temperature() {
    let mut body = tools_body();
    resolve_sampling(
        &mut body,
        &measured_ctx(temp(0.9), false),
        &agentic_layers(),
    );
    assert_param(&body, "temperature", 0.9); // measured: stands

    let mut body = tools_body();
    resolve_sampling(
        &mut body,
        &auto_detected_ctx(temp(0.9), false),
        &agentic_layers(),
    );
    assert_param(&body, "temperature", 0.3); // the same value as a guess: capped
}

/// Only the model rung is exempt. A measured recipe that names no
/// temperature resolves it from the floor, and nobody measured the floor.
#[test]
fn a_measured_model_resolving_temperature_from_the_floor_is_still_capped() {
    let mut body = tools_body();
    let recipe = InferenceConfig {
        top_k: Some(20),
        ..Default::default()
    };
    resolve_sampling(&mut body, &measured_ctx(recipe, false), &agentic_layers());
    assert_param(&body, "temperature", 0.3); // floor 0.7, capped as ever
}

/// The below-global rung names its occupant, so the debug line and the
/// audit's provenance strings stop crediting gglib's guess for a
/// measurement — or for a model author's published recipe, which the
/// static label was already misnaming.
#[test]
fn the_below_global_rung_is_named_for_its_origin() {
    let mut body = json!({});
    let decision = resolve_sampling(
        &mut body,
        &measured_ctx(temp(0.9), false),
        &SamplingLayers::default(),
    );
    assert_eq!(decision.layer_names[5], "model (measured)");

    let mut body = json!({});
    let ctx = ModelContext {
        defaults_origin: Some(DefaultsOrigin::Published),
        ..auto_detected_ctx(temp(0.9), false)
    };
    let decision = resolve_sampling(&mut body, &ctx, &SamplingLayers::default());
    assert_eq!(decision.layer_names[5], "model (published)");
}

// ── Model defaults provenance (Model.defaults_origin) ───────────────────

/// A user's own global settings must win over gglib's unreviewed guess —
/// this is the actual regression this feature exists for. Without it, a
/// `reasoning`-tagged model's auto-written recipe always wins over
/// anything configured globally, with no way to tell the two apart in
/// the resolved output.
#[test]
fn an_auto_detected_models_recipe_ranks_below_global_settings() {
    let mut body = json!({});
    let ctx = ModelContext {
        inference_defaults: Some(InferenceConfig::reasoning_profile()), // temp 1.0, presence 1.5
        defaults_origin: Some(DefaultsOrigin::AutoDetected),
        ..ModelContext::passthrough()
    };
    let layers = SamplingLayers {
        global: Some(InferenceConfig {
            temperature: Some(0.2),
            top_k: Some(20),
            min_p: Some(0.05),
            ..Default::default()
        }),
        ..Default::default()
    };
    resolve_sampling(&mut body, &ctx, &layers);

    assert_param(&body, "temperature", 0.2); // global beats the auto-detected guess
    assert_param(&body, "top_k", 20.0);
    assert_param(&body, "min_p", 0.05);
    // The claiming layer (global) left presence_penalty unset, so nothing
    // is asserted — and in particular never the auto-detected model's
    // 1.5, which was tuned for a temperature global didn't choose. The
    // provenance still reports the coupling rule rather than a plain
    // absence; see `resolve_layers_with_sources`.
    assert_deferred(&body, "presence_penalty");
}

/// The same model, but with a deliberate per-model choice instead of an
/// auto-detected one: it keeps outranking global settings exactly as
/// before this feature existed — that is what "per-model" is supposed
/// to mean.
#[test]
fn a_user_set_models_recipe_still_beats_global_settings() {
    let mut body = json!({});
    let ctx = ModelContext {
        inference_defaults: Some(InferenceConfig::reasoning_profile()),
        defaults_origin: Some(DefaultsOrigin::User),
        ..ModelContext::passthrough()
    };
    let layers = SamplingLayers {
        global: Some(InferenceConfig {
            temperature: Some(0.2),
            ..Default::default()
        }),
        ..Default::default()
    };
    resolve_sampling(&mut body, &ctx, &layers);

    assert_param(&body, "temperature", 1.0); // the user's own choice wins
    assert_param(&body, "presence_penalty", 1.5); // travels with it, intact
}

/// The force-insert. An `or_insert` implementation passes every test above
/// and fails this one. Trusted explicitly: this test is about force-insert
/// semantics, not about the trust gate.
#[test]
fn resolution_overwrites_a_partial_client_value_from_lower_layers() {
    // The client named only `temperature`. Every other key must still be
    // written from the layers beneath it rather than left absent.
    let mut body = json!({"temperature": 0.11});
    resolve_sampling(
        &mut body,
        &model_ctx(Some(InferenceConfig {
            top_p: Some(0.42),
            ..Default::default()
        })),
        &SamplingLayers {
            trust_client_sampling: true,
            ..Default::default()
        },
    );

    assert_param(&body, "temperature", 0.11);
    assert_param(&body, "top_p", 0.42);
}

// ── The agentic-turn temperature ceiling ──────────────────────────────

fn tools_body() -> Value {
    json!({"tools": [{"function": {"name": "read_file"}}]})
}

fn agentic_layers() -> SamplingLayers {
    SamplingLayers {
        agentic_adjustments: true,
        ..Default::default()
    }
}

/// A model whose defaults were written automatically at import, which is
/// what every `reasoning`-tagged model gets.
fn auto_detected_ctx(defaults: InferenceConfig, reasoning: bool) -> ModelContext {
    ModelContext {
        tags: if reasoning {
            vec!["reasoning".to_owned()]
        } else {
            Vec::new()
        },
        inference_defaults: Some(defaults),
        defaults_origin: Some(DefaultsOrigin::AutoDetected),
        ..ModelContext::passthrough()
    }
}

/// The measured decision this file used to assert the opposite of: a
/// `reasoning` model's recipe temperature stands on agentic turns.
///
/// The `0.6` cap this replaces was compared against the uncapped recipe
/// on 2026-08-10 (tune runs #12–#32, 20 paired runs): uncapped won the
/// composite (Wilcoxon one-sided p = 0.0099), tool-call formatting never
/// degraded (100% vs 98.6%), and the cap *raised* loop-guard triggers —
/// the exact failure it risked manufacturing. See
/// `agentic_temperature_ceiling` and ADR 0004's postscript.
#[test]
fn a_reasoning_models_recipe_stands_uncapped_on_agentic_turns() {
    let mut body = tools_body();
    let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
    let decision = resolve_sampling(&mut body, &ctx, &agentic_layers());

    assert!(decision.agentic_turn, "still an agentic turn");
    assert_eq!(decision.agentic_ceiling_applied, None, "no cap fired");
    assert_param(&body, "temperature", 1.0);
    // The recipe travels whole: the penalty tuned for 1.0 stays with it.
    assert_param(&body, "presence_penalty", 1.5);
}

/// Only the non-reasoning class still has a ceiling; its `0.3` predates
/// the experiment above, is unmeasured, and stands until it earns the
/// same treatment.
#[test]
fn only_non_reasoning_models_are_capped() {
    for (reasoning, expected) in [(true, 1.0), (false, 0.3)] {
        let mut body = tools_body();
        let ctx = auto_detected_ctx(
            InferenceConfig {
                temperature: Some(1.0),
                ..Default::default()
            },
            reasoning,
        );
        resolve_sampling(&mut body, &ctx, &agentic_layers());

        assert_param(&body, "temperature", expected);
    }
}

/// Deliberate configuration outranks the ceiling. This is the whole
/// reason the gate is provenance rather than rank.
#[test]
fn a_deliberate_temperature_is_never_capped() {
    let mut body = tools_body();
    // `User` origin, so the recipe occupies the per-model rung rather than
    // the auto-detected one.
    let ctx = ModelContext {
        inference_defaults: Some(temp(0.9)),
        defaults_origin: Some(DefaultsOrigin::User),
        ..ModelContext::passthrough()
    };
    resolve_sampling(&mut body, &ctx, &agentic_layers());

    assert_param(&body, "temperature", 0.9);
}

/// The regression guard for #743: the adjustment must not disable DRY.
#[test]
fn dry_survives_an_agentic_turn() {
    let mut body = tools_body();
    let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
    resolve_sampling(
        &mut body,
        &ctx,
        &SamplingLayers {
            agentic_adjustments: true,
            global: Some(InferenceConfig {
                temperature: Some(0.8),
                dry_multiplier: Some(0.8),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    assert_param(&body, "dry_multiplier", 0.8);
    // Global is a deliberate setting, so its temperature stands uncapped.
    assert_param(&body, "temperature", 0.8);
}

/// The entropy-adaptive fields ride the same uncoupled rules DRY does: a
/// layer naming only `dynatemp_range` keeps it when a lower layer claims
/// the temperature, and nothing is emitted for them when no layer names
/// one — llama.cpp's own "off" defaults apply by silence, per ADR 0003.
#[test]
fn dynatemp_survives_without_a_temperature_in_the_same_layer_and_defers_otherwise() {
    // Nothing names the entropy-adaptive fields: they must not reach the
    // wire at all.
    let mut body = json!({});
    let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
    resolve_sampling(&mut body, &ctx, &SamplingLayers::default());
    assert_deferred(&body, "dynatemp_range");
    assert_deferred(&body, "dynatemp_exponent");
    assert_deferred(&body, "top_n_sigma");

    // A profile naming only the entropy-adaptive fields keeps them even
    // though the model's auto-detected recipe claims the temperature.
    let mut body = json!({});
    resolve_sampling(
        &mut body,
        &ctx,
        &SamplingLayers {
            profile: Some(InferenceConfig {
                dynatemp_range: Some(0.5),
                top_n_sigma: Some(1.0),
                ..Default::default()
            }),
            ..Default::default()
        },
    );
    assert_param(&body, "dynatemp_range", 0.5);
    assert_param(&body, "top_n_sigma", 1.0);
    // The trio still comes from the claiming layer, untouched.
    assert_param(&body, "temperature", 1.0);
    assert_param(&body, "presence_penalty", 1.5);
}

/// Regression guard for #745. DRY is deliberately *not* part of the
/// temperature-coupled trio, so a layer naming a DRY value but no
/// temperature keeps it even though a lower layer claims the temperature —
/// which is the default state of every `reasoning`-tagged model.
#[test]
fn dry_survives_without_a_temperature_in_the_same_layer() {
    let mut body = json!({});
    // The model's auto-detected recipe names a temperature, so it claims
    // the trio. The profile names only DRY.
    let ctx = auto_detected_ctx(InferenceConfig::reasoning_profile(), true);
    resolve_sampling(
        &mut body,
        &ctx,
        &SamplingLayers {
            profile: Some(InferenceConfig {
                dry_multiplier: Some(0.8),
                ..Default::default()
            }),
            ..Default::default()
        },
    );

    assert_param(&body, "dry_multiplier", 0.8);
    // The trio still comes from the claiming layer, untouched by this.
    assert_param(&body, "presence_penalty", 1.5);
    assert_param(&body, "temperature", 1.0);
}

/// The ceiling only ever lowers. A model already below it is untouched.
#[test]
fn the_ceiling_never_raises_a_temperature() {
    let mut body = tools_body();
    let ctx = auto_detected_ctx(temp(0.1), false);
    resolve_sampling(&mut body, &ctx, &agentic_layers());

    assert_param(&body, "temperature", 0.1);
}

/// `top_p` is left alone when the cap fires. The floor this replaced
/// forced it to 1.0, which contradicted published model guidance.
#[test]
fn the_ceiling_does_not_touch_top_p() {
    let mut body = tools_body();
    let ctx = auto_detected_ctx(
        InferenceConfig {
            temperature: Some(1.0),
            top_p: Some(0.95),
            ..Default::default()
        },
        false,
    );
    resolve_sampling(&mut body, &ctx, &agentic_layers());

    assert_param(&body, "temperature", 0.3);
    assert_param(&body, "top_p", 0.95);
}

#[test]
fn a_request_without_tools_is_never_capped() {
    let mut body = json!({});
    let ctx = auto_detected_ctx(temp(1.0), false);
    resolve_sampling(&mut body, &ctx, &agentic_layers());

    assert_param(&body, "temperature", 1.0);
}

/// `strip_unsupported_tools` leaves a dangling `tool_choice` when there
/// were no tools to strip, so this shape reaches stage 4 in practice.
#[test]
fn a_dangling_tool_choice_without_tools_is_not_an_agentic_turn() {
    let mut body = json!({"tool_choice": "required"});
    let ctx = auto_detected_ctx(temp(1.0), false);
    resolve_sampling(&mut body, &ctx, &agentic_layers());

    assert_param(&body, "temperature", 1.0);
}

#[test]
fn the_ceiling_does_nothing_when_the_caller_has_not_enabled_it() {
    let mut body = tools_body();
    let ctx = auto_detected_ctx(temp(1.0), false);
    resolve_sampling(&mut body, &ctx, &SamplingLayers::default());

    assert_param(&body, "temperature", 1.0);
}

// ── cache_prompt ──────────────────────────────────────────────────────

#[test]
fn cache_prompt_is_pinned_true_when_absent() {
    let mut body = json!({});
    resolve_sampling(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
    );
    assert_eq!(body["cache_prompt"], true);
}

/// The KV cache feature depends on this: a client that sends `false` must
/// not be able to discard the whole restored cache.
#[test]
fn cache_prompt_is_forced_true_over_an_explicit_false() {
    let mut body = json!({"cache_prompt": false});
    resolve_sampling(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
    );
    assert_eq!(body["cache_prompt"], true);
}

// ── Passthrough ───────────────────────────────────────────────────────

#[test]
fn unknown_fields_survive_untouched() {
    let mut body = json!({
        "model": "m",
        "messages": [{"role": "user", "content": "hi"}],
        "totally_made_up_key": {"nested": [1, 2, {"deep": true}]},
    });
    resolve_sampling(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
    );

    assert_eq!(body["model"], "m");
    assert_eq!(body["messages"][0]["content"], "hi");
    assert_eq!(
        body["totally_made_up_key"],
        json!({"nested": [1, 2, {"deep": true}]})
    );
}

/// `max_tokens` has no hardcoded fallback on purpose — a value here would
/// cap every request that did not name its own.
#[test]
fn no_max_tokens_is_written_when_nothing_sets_one() {
    let mut body = json!({});
    resolve_sampling(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
    );
    assert!(body.as_object().unwrap().get("max_tokens").is_none());
}

#[test]
fn a_non_object_body_is_left_alone() {
    let mut body = json!([1, 2, 3]);
    resolve_sampling(
        &mut body,
        &ModelContext::passthrough(),
        &SamplingLayers::default(),
    );
    assert_eq!(body, json!([1, 2, 3]));
}
