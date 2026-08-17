//! Tests for [`super`] — including the exhaustiveness guard that stops a
//! newly modelled parameter from being settable but not clearable.

use super::*;
use gglib_core::domain::ReasoningEffort;

/// Every field set, written without `..Default::default()` on purpose: adding
/// a field to `InferenceConfig` must break this literal, which is what makes
/// [`every_modelled_parameter_is_clearable`] a real guard rather than a
/// snapshot of the fields that happened to exist when it was written.
fn fully_populated() -> InferenceConfig {
    InferenceConfig {
        temperature: Some(0.2),
        top_p: Some(0.9),
        top_k: Some(40),
        max_tokens: Some(2048),
        repeat_penalty: Some(1.1),
        presence_penalty: Some(1.5),
        frequency_penalty: Some(0.1),
        min_p: Some(0.05),
        dry_multiplier: Some(0.8),
        dry_base: Some(1.75),
        dry_allowed_length: Some(2),
        dry_penalty_last_n: Some(64),
        dynatemp_range: Some(0.5),
        dynatemp_exponent: Some(1.0),
        top_n_sigma: Some(2.0),
        seed: Some(7),
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(4096),
    }
}

/// The camelCase wire name of a field, as its `--unset` argument.
fn flag_name(wire_key: &str) -> String {
    let mut out = String::new();
    for c in wire_key.chars() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A parameter that can be *set* from a surface and not *cleared* from it is a
/// one-way door: the stored value can be changed but never taken back out of
/// the layer. Derived from serde rather than listed, so it covers whatever
/// `InferenceConfig` models today.
#[test]
fn every_modelled_parameter_is_clearable() {
    let json = serde_json::to_value(fully_populated()).expect("serializes");
    let fields = json.as_object().expect("an object");
    assert!(fields.len() > 15, "sanity: got {} fields", fields.len());

    for key in fields.keys() {
        let flag = flag_name(key);
        let mut config = fully_populated();
        let result = clear_param(&mut config, &flag);

        if flag == "seed" {
            // No surface stores a seed, so none can clear one. Stated here so
            // the exemption is visible rather than an unexplained gap.
            assert!(result.is_err(), "seed is deliberately not clearable");
            continue;
        }

        result.unwrap_or_else(|e| panic!("--unset {flag} should be accepted: {e}"));
        let after = serde_json::to_value(&config).expect("serializes");
        assert!(
            after[key].is_null(),
            "--unset {flag} left {key} = {}",
            after[key]
        );
    }
}

/// Clearing one parameter must not disturb the rest — that is the whole
/// difference between `--unset` and `--clear-inference-defaults`.
#[test]
fn clearing_one_parameter_leaves_the_others_alone() {
    let mut config = fully_populated();
    clear_param(&mut config, "reasoning-effort").expect("accepted");

    assert_eq!(config.reasoning_effort, None);
    assert_eq!(
        config.reasoning_budget_tokens,
        Some(4096),
        "the effort's twin is a separate parameter and stays put"
    );
    assert_eq!(config.temperature, Some(0.2));
}

#[test]
fn accepts_both_spellings() {
    let mut hyphen = fully_populated();
    clear_param(&mut hyphen, "reasoning-budget-tokens").expect("hyphenated form");
    assert_eq!(hyphen.reasoning_budget_tokens, None);

    let mut underscore = fully_populated();
    clear_param(&mut underscore, "reasoning_budget_tokens").expect("underscored form");
    assert_eq!(underscore.reasoning_budget_tokens, None);
}

#[test]
fn rejects_an_unknown_name_and_lists_what_is_accepted() {
    let err = clear_param(&mut fully_populated(), "nonsense").expect_err("should reject");
    let message = err.to_string();
    assert!(message.contains("nonsense"), "got: {message}");
    assert!(message.contains("temperature"), "got: {message}");
    assert!(message.contains("reasoning-effort"), "got: {message}");
}
