//! The stored layers a caller hands the adapter: folded once, by the shared
//! pipeline, beneath what a person chose for the turn.
//!
//! The ladder's rules are tested in `gglib_core::request_pipeline`. These
//! assert that the adapter gives that fold everything it was handed and
//! resolves nothing itself, which is what lets the pipeline tell a value a
//! person chose from one nobody did, and that the request it sends carries
//! what the fold resolved and no parameter beside it.

use std::sync::Mutex;

use gglib_core::domain::agent::ToolDefinition;
use gglib_core::domain::{DefaultsOrigin, ModelSamplingContext};

use super::*;

fn temperature(value: f32) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(value),
        ..InferenceConfig::default()
    }
}

/// A model in the catalogue that can call tools, with `defaults` stored as
/// `origin` says they were.
fn model(defaults: Option<(InferenceConfig, DefaultsOrigin)>) -> ModelContext {
    let (inference_defaults, defaults_origin) = defaults.unzip();
    ModelContext {
        capabilities: ModelCapabilities::SUPPORTS_TOOL_CALLS
            | ModelCapabilities::SUPPORTS_SYSTEM_ROLE,
        inference_defaults,
        defaults_origin,
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

fn layers(profile: Option<f32>, global: Option<f32>) -> SamplingLayers {
    SamplingLayers {
        profile: profile.map(temperature),
        global: global.map(temperature),
        ..SamplingLayers::default()
    }
}

/// The temperature a turn is sent with, `tools` saying whether it carries one.
fn sent(adapter: &LlmCompletionAdapter, tools: bool) -> f64 {
    let tool = ToolDefinition {
        name: "f".to_owned(),
        description: None,
        input_schema: Some(json!({"type": "object"})),
        title: None,
    };
    let tools = if tools { vec![tool] } else { Vec::new() };
    let body = adapter
        .shaped_body(&[user("hi")], &tools, &ImageUrls::default())
        .unwrap();
    let sent = body["temperature"].as_f64().expect("a temperature");
    (sent * 100.0).round() / 100.0
}

/// The settings' global layer is the adapter's to fold: it reaches the body
/// when nothing above it names a temperature, beneath a value a person set on
/// the model and above one gglib guessed at import.
#[test]
fn the_global_layer_is_folded_between_a_set_model_value_and_a_guessed_one() {
    let with = |ctx: ModelContext| adapter(ctx, None).with_layers(layers(None, Some(0.42)));

    assert!((sent(&with(model(None)), false) - 0.42).abs() < 1e-9);
    let set = model(Some((temperature(0.55), DefaultsOrigin::User)));
    assert!((sent(&with(set), false) - 0.55).abs() < 1e-9);
    let guessed = model(Some((temperature(0.9), DefaultsOrigin::AutoDetected)));
    assert!((sent(&with(guessed), false) - 0.42).abs() < 1e-9);
}

/// The selected profile outranks the model's own value and the global one,
/// and what a person chose for the turn outranks the profile.
#[test]
fn the_profile_layer_outranks_the_model_and_the_callers_own_value_outranks_it() {
    let set = || model(Some((temperature(0.55), DefaultsOrigin::User)));
    let stored = layers(Some(0.15), Some(0.42));

    let profiled = adapter(set(), None).with_layers(stored.clone());
    assert!((sent(&profiled, false) - 0.15).abs() < 1e-9);
    let typed = adapter(set(), Some(temperature(0.9))).with_layers(stored);
    assert!((sent(&typed, false) - 0.9).abs() < 1e-9);
}

/// On a turn with tools the ceiling lowers a temperature nobody chose, the
/// floor's or an import-time guess, and never one a person did: typed for
/// the turn, a profile's, set on the model, or the global one. That only
/// holds because each arrives as the layer it is.
#[test]
fn a_turn_with_tools_caps_a_temperature_nobody_chose_and_no_other() {
    let floor = adapter(model(None), None);
    assert!((sent(&floor, false) - 0.7).abs() < 1e-9, "no tools, no cap");
    assert!((sent(&floor, true) - 0.3).abs() < 1e-9);
    let guessed = model(Some((temperature(0.9), DefaultsOrigin::AutoDetected)));
    assert!((sent(&adapter(guessed, None), true) - 0.3).abs() < 1e-9);

    let typed = adapter(model(None), Some(temperature(0.9)));
    assert!((sent(&typed, true) - 0.9).abs() < 1e-9);
    let profile = adapter(model(None), None).with_layers(layers(Some(0.9), None));
    assert!((sent(&profile, true) - 0.9).abs() < 1e-9);
    let set = model(Some((temperature(0.9), DefaultsOrigin::User)));
    assert!((sent(&adapter(set, None), true) - 0.9).abs() < 1e-9);
    let global = adapter(model(None), None).with_layers(layers(None, Some(0.9)));
    assert!((sent(&global, true) - 0.9).abs() < 1e-9);
}

/// The sampling parameters a request carries, as the server reads them.
fn sampling_of(body: &Value) -> InferenceConfig {
    InferenceConfig::extract_client_sampling(body).0
}

fn penalty() -> InferenceConfig {
    InferenceConfig {
        presence_penalty: Some(1.2),
        ..InferenceConfig::default()
    }
}

/// A penalty a person named without the temperature it travels with is
/// passed over by the fold when a layer beneath names one, and is not sent:
/// the request carries what its decision says. Named with its temperature,
/// or over layers that name none, it is what the turn runs with.
#[test]
fn a_parameter_the_fold_passes_over_is_not_sent() {
    let set = || model(Some((temperature(0.55), DefaultsOrigin::User)));

    let by_model = body_of(&adapter(set(), Some(penalty())), &[user("hi")]);
    assert_eq!(sampling_of(&by_model), temperature(0.55), "{by_model}");
    let profiled = adapter(model(None), Some(penalty())).with_layers(layers(Some(0.15), None));
    let by_profile = body_of(&profiled, &[user("hi")]);
    assert_eq!(sampling_of(&by_profile), temperature(0.15), "{by_profile}");

    let with_its_temperature = InferenceConfig {
        temperature: Some(0.5),
        ..penalty()
    };
    let typed = adapter(set(), Some(with_its_temperature.clone()));
    let typed = body_of(&typed, &[user("hi")]);
    assert_eq!(sampling_of(&typed), with_its_temperature, "{typed}");
    let unclaimed = body_of(&adapter(model(None), Some(penalty())), &[user("hi")]);
    assert_eq!(unclaimed["presence_penalty"], json!(1.2_f32), "{unclaimed}");
}

/// A value the pipeline's reader refuses is not the fold's to pass over: it
/// stays in the request for llama-server to answer, as the pipeline leaves
/// an external client's.
#[test]
fn a_parameter_the_reader_refuses_is_left_for_the_server_to_answer() {
    let below_the_range = InferenceConfig {
        reasoning_budget_tokens: Some(-2),
        ..InferenceConfig::default()
    };
    let body = body_of(&adapter(model(None), Some(below_the_range)), &[user("hi")]);
    assert_eq!(body["reasoning_budget_tokens"], json!(-2), "{body}");
}

/// On a turn with no tools the adapter sends what the stored ladder resolves,
/// the fold `gglib model explain` reports, for the same typed values,
/// profile, model and global defaults, and no parameter beside it: every
/// combination below, each request compared whole.
#[test]
fn a_turn_with_no_tools_sends_what_the_stored_ladder_resolves_and_no_more() {
    let config = |edit: fn(&mut InferenceConfig)| {
        let mut config = InferenceConfig::default();
        edit(&mut config);
        config
    };
    let typed = [
        None,
        Some(temperature(0.9)),
        Some(penalty()),
        Some(config(|c| c.repeat_penalty = Some(1.1))),
        Some(config(|c| c.min_p = Some(0.05))),
        Some(config(|c| c.top_k = Some(10))),
        Some(InferenceConfig {
            temperature: Some(0.2),
            ..penalty()
        }),
    ];
    let profiles = [
        None,
        Some(temperature(0.15)),
        Some(config(|c| c.presence_penalty = Some(0.3))),
    ];
    let stored = [
        None,
        Some((temperature(0.55), DefaultsOrigin::User)),
        Some((config(|c| c.min_p = Some(0.1)), DefaultsOrigin::User)),
        Some((
            InferenceConfig::reasoning_profile(),
            DefaultsOrigin::AutoDetected,
        )),
        Some((temperature(0.6), DefaultsOrigin::Measured)),
    ];
    let globals = [
        None,
        Some(temperature(0.42)),
        Some(config(|c| {
            c.top_k = Some(33);
            c.repeat_penalty = Some(1.05);
        })),
    ];

    for named in &typed {
        for profile in &profiles {
            for defaults in &stored {
                for global in &globals {
                    for reasoning in [false, true] {
                        let (own, defaults_origin) = defaults.clone().unzip();
                        let explained = named.clone().unwrap_or_default().resolve_with_profile(
                            profile.as_ref(),
                            own.as_ref(),
                            global.as_ref(),
                            ModelSamplingContext {
                                is_reasoning: reasoning,
                                defaults_origin,
                            },
                        );
                        let ctx = ModelContext {
                            tags: reasoning
                                .then(|| "reasoning".to_owned())
                                .into_iter()
                                .collect(),
                            ..model(defaults.clone())
                        };
                        let adapter = adapter(ctx, named.clone()).with_layers(SamplingLayers {
                            profile: profile.clone(),
                            global: global.clone(),
                            ..SamplingLayers::default()
                        });
                        let body = body_of(&adapter, &[user("hi")]);
                        assert_eq!(
                            sampling_of(&body),
                            explained,
                            "typed {named:?}, profile {profile:?}, model {defaults:?}, \
                             global {global:?}, reasoning {reasoning}: {body}"
                        );
                    }
                }
            }
        }
    }
}

/// The observer is told the decision each request was sampled by, and that
/// decision names the rung a value came from: `global` for the global layer,
/// never the caller's own rung, which is where a value folded before the
/// adapter would land.
#[test]
fn the_observer_is_told_each_requests_decision_with_the_rung_that_supplied_it() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let told = Arc::clone(&seen);
    let adapter = adapter(model(None), Some(temperature(0.9)))
        .with_layers(SamplingLayers {
            global: Some(InferenceConfig {
                top_k: Some(33),
                ..temperature(0.42)
            }),
            ..SamplingLayers::default()
        })
        .with_sampling_observer(Some(Arc::new(move |decision: &SamplingDecision| {
            told.lock().unwrap().push(decision.clone());
        })));

    body_of(&adapter, &[user("hi")]);
    body_of(&adapter, &[user("again")]);

    let seen = std::mem::take(&mut *seen.lock().unwrap());
    assert_eq!(seen.len(), 2, "one decision a request");
    let from = seen[0].sources.describe(&seen[0].layer_names);
    assert_eq!(seen[0].resolved.temperature, Some(0.9));
    assert!(from.contains("temperature=client"), "{from}");
    assert_eq!(seen[0].resolved.top_k, Some(33));
    assert!(from.contains("top_k=global"), "{from}");
}
