//! Which flags the warning names, read off the decision a request was
//! sampled by, and that a session says it once.

use std::sync::Mutex;

use gglib_core::domain::{DefaultsOrigin, ReasoningEffort, TemplateCaps};
use gglib_core::request_pipeline::{self, ModelContext, SamplingLayers};
use serde_json::{Value, json};

use super::*;

const PENALTY_PASSED_OVER: &str = "  Warning: --presence-penalty did not take effect. Sampling \
     penalties travel with whichever layer sets the temperature; pass --temperature to set \
     them together.";

fn temperature(value: f32) -> InferenceConfig {
    InferenceConfig {
        temperature: Some(value),
        ..InferenceConfig::default()
    }
}

fn penalty() -> InferenceConfig {
    InferenceConfig {
        presence_penalty: Some(1.2),
        ..InferenceConfig::default()
    }
}

/// A model of this catalogue with `defaults` as gglib guessed them at import.
fn model(defaults: Option<InferenceConfig>) -> ModelContext {
    ModelContext {
        defaults_origin: defaults.as_ref().map(|_| DefaultsOrigin::AutoDetected),
        inference_defaults: defaults,
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

fn profile(config: InferenceConfig) -> SamplingLayers {
    SamplingLayers {
        profile: Some(config),
        ..SamplingLayers::default()
    }
}

/// The decision the pipeline makes for a turn with `flags` typed, as the
/// adapter asks for it: the flags in the body, the stored layers beside it.
fn decided(
    flags: &InferenceConfig,
    ctx: &ModelContext,
    stored: SamplingLayers,
) -> SamplingDecision {
    let mut body = Value::Object(flags.to_openai_json_patch());
    body["messages"] = json!([{ "role": "user", "content": "hi" }]);
    let layers = SamplingLayers {
        trust_client_sampling: true,
        agentic_adjustments: true,
        ..stored
    };
    request_pipeline::apply(&mut body, ctx, &layers, None)
        .expect("the pipeline applies")
        .sampling
}

/// `--profile chat --presence-penalty 1.2`, and the same flag on a model
/// whose stored defaults name a temperature: the penalty travels with the
/// temperature, which the flags did not set.
#[test]
fn a_bare_penalty_is_named_when_a_profile_or_the_model_sets_the_temperature() {
    let by_profile = decided(&penalty(), &model(None), profile(temperature(0.8)));
    assert_eq!(
        warning(&penalty(), &by_profile).as_deref(),
        Some(PENALTY_PASSED_OVER)
    );

    let stored = model(Some(temperature(1.0)));
    let by_model = decided(&penalty(), &stored, SamplingLayers::default());
    assert_eq!(
        warning(&penalty(), &by_model).as_deref(),
        Some(PENALTY_PASSED_OVER)
    );
}

/// A penalty typed with its temperature, or over layers that name none, is
/// what the turn runs with: nothing to say.
#[test]
fn a_flag_that_took_effect_is_not_named() {
    let both = InferenceConfig {
        temperature: Some(0.5),
        ..penalty()
    };
    let with_temperature = decided(&both, &model(None), profile(temperature(0.8)));
    assert_eq!(with_temperature.resolved.presence_penalty, Some(1.2));
    assert_eq!(warning(&both, &with_temperature), None);

    let nothing_claims_it = decided(&penalty(), &model(None), SamplingLayers::default());
    assert_eq!(nothing_claims_it.resolved.presence_penalty, Some(1.2));
    assert_eq!(warning(&penalty(), &nothing_claims_it), None);
}

/// An effort level the model's template does not read is deleted by that
/// gate. The sentence explains the temperature's coupling, which had no part
/// in it, so the level is not named; a penalty lost beside it still is.
#[test]
fn an_effort_level_the_template_does_not_read_is_not_blamed_on_the_temperature() {
    let unread = ModelContext {
        template_caps: Some(TemplateCaps {
            supports_reasoning_effort: Some(false),
            ..TemplateCaps::default()
        }),
        ..model(None)
    };
    let effort = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        ..InferenceConfig::default()
    };
    let dropped = decided(&effort, &unread, SamplingLayers::default());
    assert_eq!(
        dropped.sources.reasoning_effort,
        ParamSource::SuppressedByTemplate
    );
    assert_eq!(warning(&effort, &dropped), None);

    let with_penalty = InferenceConfig {
        reasoning_effort: Some(ReasoningEffort::High),
        ..penalty()
    };
    let both = decided(&with_penalty, &unread, profile(temperature(0.8)));
    assert_eq!(
        warning(&with_penalty, &both).as_deref(),
        Some(PENALTY_PASSED_OVER)
    );
}

/// The flags are read back at the rung the pipeline names `client`.
#[test]
fn the_flags_rung_is_the_one_the_pipeline_reads_the_callers_parameters_at() {
    let decision = decided(&temperature(0.5), &model(None), SamplingLayers::default());
    assert_eq!(decision.layer_names[FLAGS_RUNG], "client");
    assert_eq!(decision.sources.temperature, ParamSource::Layer(FLAGS_RUNG));
}

/// The first request of a session says it, and no later one does again.
#[test]
fn a_session_is_told_once() {
    let said = Arc::new(Mutex::new(Vec::new()));
    let heard = Arc::clone(&said);
    let observer = telling(penalty(), move |warning| {
        heard.lock().unwrap().push(warning.to_owned());
    });
    let decision = decided(&penalty(), &model(None), profile(temperature(0.8)));

    observer(&decision);
    observer(&decision);

    assert_eq!(*said.lock().unwrap(), [PENALTY_PASSED_OVER]);
}
