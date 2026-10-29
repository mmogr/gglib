//! The defaults against what the pinned llama-server reported, and every
//! sampling key gglib's proxy sends read.

use gglib_core::domain::{InferenceConfig, ReasoningEffort};
use serde_json::{Value, json};

use super::Sampling;

/// What the pinned llama-server's `/props` answered.
const PROPS: &str =
    include_str!("../../../../contracts/upstream/llama/bodies/props.Qwen-Qwen3-0.6B.json");

/// Whether two JSON numbers are the same `f32`.
fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() < 1e-6,
        _ => a == b,
    }
}

#[test]
fn the_defaults_are_what_the_pinned_llama_server_samples_with() {
    let props: Value = serde_json::from_str(PROPS).expect("the recorded /props");
    let recorded = &props["default_generation_settings"]["params"];
    let ours = Sampling::default().to_json();
    for (key, value) in ours.as_object().expect("an object") {
        assert_eq!(&recorded[key], value, "{key}");
    }
}

/// Every field the proxy's sampling stage can write, each away from
/// llama-server's default. A field added to `InferenceConfig` fails to
/// compile here until it is said whether the engine reads it.
fn everything() -> InferenceConfig {
    InferenceConfig {
        temperature: Some(0.3),
        top_p: Some(0.7),
        top_k: Some(11),
        max_tokens: Some(99),
        repeat_penalty: Some(1.1),
        presence_penalty: Some(0.4),
        frequency_penalty: Some(0.2),
        min_p: Some(0.02),
        dynatemp_range: Some(0.5),
        dynatemp_exponent: Some(1.5),
        top_n_sigma: Some(1.2),
        dry_multiplier: Some(0.8),
        dry_base: Some(1.9),
        dry_allowed_length: Some(3),
        dry_penalty_last_n: Some(128),
        seed: Some(7),
        // These two shape the prompt, not the sampler: `render_tests.rs`.
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(0),
    }
}

#[test]
fn every_sampling_key_gglibs_proxy_sends_is_read() {
    let patch = everything().to_openai_json_patch();
    let body = Value::Object(patch.clone());
    let read = Sampling::from_request(&body).to_json();
    let defaults = Sampling::default().to_json();
    let mut checked = 0;
    for (key, sent) in &patch {
        if key.starts_with("reasoning_") {
            continue;
        }
        assert!(
            same(&read[key], sent),
            "{key}: sent {sent}, read {}",
            read[key]
        );
        assert!(!same(&defaults[key], sent), "{key} is sent at its default");
        checked += 1;
    }
    assert_eq!(checked, 16, "{patch:?}");
}

#[test]
fn a_key_of_the_wrong_type_keeps_its_default() {
    let body = json!({"temperature": "hot", "top_k": null, "seed": "lucky", "ignore_eos": 1});
    assert_eq!(Sampling::from_request(&body), Sampling::default());
}

#[test]
fn n_predict_beats_max_tokens_and_a_float_is_truncated() {
    let read = Sampling::from_request(&json!({"max_tokens": 5, "n_predict": 9}));
    assert_eq!(read.n_predict, 9);
    assert_eq!(Sampling::from_request(&json!({"top_k": 7.9})).top_k, 7);
}

#[test]
fn a_seed_of_minus_one_is_drawn() {
    let read = Sampling::from_request(&json!({"seed": -1}));
    assert_eq!(read.seed, u32::MAX);
    assert_eq!(Sampling::from_request(&json!({"seed": 42})).seed, 42);
}
