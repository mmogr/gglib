//! Non-finite values: named by the caller, impossible to send.
//!
//! Split from `sampling_discards_tests.rs` because it asks a different
//! question. Those tests are about which *rung* won; these are about a value
//! that cannot reach llama-server whichever rung wins, because JSON has no NaN
//! or infinity and `to_openai_json_patch` therefore drops it.

use super::*;

use crate::domain::ModelSamplingContext;

/// The stored ladder: `[request, profile, model, global, model auto]`.
const REQUEST_RUNG: usize = 0;

fn resolve(
    request: &InferenceConfig,
    profile: Option<&InferenceConfig>,
    model: Option<&InferenceConfig>,
) -> (InferenceConfig, FieldSources) {
    request.clone().resolve_with_profile_explained(
        profile,
        model,
        None,
        ModelSamplingContext::default(),
    )
}

/// The false negative this closes: JSON has no NaN, so
/// `to_openai_json_patch` drops the field entirely and the membership test
/// used to skip it before the coupling rule was ever consulted. The user
/// passed a value, nothing was sent, and nothing said so.
#[test]
fn a_non_finite_flag_is_reported_even_though_it_never_serialises() {
    let request = InferenceConfig {
        presence_penalty: Some(f32::NAN),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.8),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, Some(&profile), None);

    assert!(
        !resolved
            .to_openai_json_patch()
            .contains_key("presence_penalty"),
        "guards the premise: the value cannot reach the wire"
    );
    assert_eq!(
        discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG),
        vec!["presence_penalty"]
    );
}

/// Infinity has the same problem as NaN, and so does a value whose rung *won*:
/// winning the ladder is not the same as reaching llama-server.
#[test]
fn a_non_finite_flag_is_reported_even_when_its_rung_won() {
    let request = InferenceConfig {
        temperature: Some(f32::INFINITY),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, None, None);

    assert_eq!(
        sources.temperature,
        ParamSource::Layer(REQUEST_RUNG),
        "guards the premise: the request rung won the ladder"
    );
    assert_eq!(
        discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG),
        vec!["temperature"],
        "winning the ladder is not the same as being sent"
    );
}

/// Negative infinity too, and only the offending field is named.
#[test]
fn a_finite_neighbour_of_a_non_finite_flag_is_not_reported() {
    let request = InferenceConfig {
        top_p: Some(f32::NEG_INFINITY),
        top_k: Some(40),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, None, None);

    assert_eq!(
        discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG),
        vec!["top_p"],
        "top_k is finite, reached the wire, and must stay quiet"
    );
}

/// Drift guard for `non_finite_fields`: every float field on
/// `InferenceConfig` must be checked, or a future one silently reopens the
/// hole. Sets each in turn to NaN and asserts it is reported.
#[test]
fn every_float_field_is_checked_for_non_finiteness() {
    let cases: Vec<(&str, InferenceConfig)> = vec![
        (
            "temperature",
            InferenceConfig {
                temperature: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "top_p",
            InferenceConfig {
                top_p: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "presence_penalty",
            InferenceConfig {
                presence_penalty: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "repeat_penalty",
            InferenceConfig {
                repeat_penalty: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "min_p",
            InferenceConfig {
                min_p: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "frequency_penalty",
            InferenceConfig {
                frequency_penalty: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "dynatemp_range",
            InferenceConfig {
                dynatemp_range: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "dynatemp_exponent",
            InferenceConfig {
                dynatemp_exponent: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "top_n_sigma",
            InferenceConfig {
                top_n_sigma: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "dry_multiplier",
            InferenceConfig {
                dry_multiplier: Some(f32::NAN),
                ..Default::default()
            },
        ),
        (
            "dry_base",
            InferenceConfig {
                dry_base: Some(f32::NAN),
                ..Default::default()
            },
        ),
    ];

    for (field, request) in cases {
        let (resolved, sources) = resolve(&request, None, None);
        assert!(
            discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG).contains(&field),
            "{field} is a float field that `non_finite_fields` does not check"
        );
    }
}

/// Drift guard with teeth. The test above only proves the eleven fields it
/// names are covered; it cannot notice a *twelfth* being added, because an
/// unset field is dropped from the patch exactly like a NaN one. Pinning the
/// count of modelled fields does notice: adding any field to `InferenceConfig`
/// fails here, which is the prompt to ask whether it is a float and belongs in
/// `non_finite_fields`.
#[test]
fn the_modelled_field_count_is_pinned() {
    let everything = InferenceConfig {
        temperature: Some(0.5),
        top_p: Some(0.5),
        top_k: Some(1),
        max_tokens: Some(1),
        presence_penalty: Some(0.5),
        repeat_penalty: Some(0.5),
        min_p: Some(0.5),
        frequency_penalty: Some(0.5),
        dynatemp_range: Some(0.5),
        dynatemp_exponent: Some(0.5),
        top_n_sigma: Some(0.5),
        dry_multiplier: Some(0.5),
        dry_base: Some(0.5),
        dry_allowed_length: Some(1),
        dry_penalty_last_n: Some(1),
        seed: Some(1),
        reasoning_effort: Some(crate::domain::ReasoningEffort::High),
        reasoning_budget_tokens: Some(1),
    };

    assert_eq!(
        everything.to_openai_json_patch().len(),
        18,
        "InferenceConfig gained or lost a field — if it is an f32, add it to \
         `non_finite_fields` and to the drift guard above"
    );
}
