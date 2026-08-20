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

#[test]
fn a_request_flag_that_won_is_not_reported() {
    let request = InferenceConfig {
        temperature: Some(0.2),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, None, None);

    assert_eq!(sources.temperature, ParamSource::Layer(REQUEST_RUNG));
    assert!(discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG).is_empty());
}

/// The motivating case: the profile claims the temperature, so the coupled
/// trio comes only from the profile — and the profile names no penalty, so the
/// caller's is gone with nothing in its place.
#[test]
fn a_coupled_flag_a_profile_passed_over_is_reported() {
    let request = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.8),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, Some(&profile), None);

    assert_eq!(sources.presence_penalty, ParamSource::FloorCoupled);
    assert_eq!(
        discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG),
        vec!["presence_penalty"]
    );
}

/// The same loss by a different route: the claiming rung supplied its own
/// value, so provenance reads `Layer(1)` rather than `FloorCoupled`. Testing
/// only for `FloorCoupled` would miss this entirely.
#[test]
fn a_coupled_flag_the_claiming_layer_overrode_is_reported() {
    let request = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.8),
        presence_penalty: Some(0.4),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, Some(&profile), None);

    assert_eq!(sources.presence_penalty, ParamSource::Layer(1));
    assert_eq!(resolved.presence_penalty, Some(0.4));
    assert_eq!(
        discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG),
        vec!["presence_penalty"]
    );
}

/// Nothing was lost, so nothing is said — even though the winning rung is not
/// the caller's. Warning here would train people to ignore the warning.
#[test]
fn an_identical_value_from_the_claiming_rung_is_not_reported() {
    let request = InferenceConfig {
        presence_penalty: Some(0.4),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.8),
        presence_penalty: Some(0.4),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, Some(&profile), None);

    assert!(discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG).is_empty());
}

/// Uncoupled parameters gap-fill independently, so the request's own value
/// always wins and is never reported.
#[test]
fn an_uncoupled_flag_is_never_reported() {
    let request = InferenceConfig {
        top_p: Some(0.5),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.8),
        top_p: Some(0.95),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, Some(&profile), None);

    assert_eq!(resolved.top_p, Some(0.5));
    assert!(discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG).is_empty());
}

/// With no layer claiming a temperature the trio gap-fills like anything else,
/// so the caller's penalty stands.
#[test]
fn nothing_is_reported_when_no_layer_claims_a_temperature() {
    let request = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let model = InferenceConfig {
        top_k: Some(40),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, None, Some(&model));

    assert_eq!(resolved.presence_penalty, Some(1.2));
    assert!(discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG).is_empty());
}

/// The hazard the rung parameter exists for: one resolution, read from two
/// rungs, must give two answers. A helper that hardcoded rung 0 would report
/// the client's losses as the operator's.
#[test]
fn the_rung_index_is_respected() {
    let request = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.8),
        presence_penalty: Some(0.4),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, Some(&profile), None);

    assert_eq!(sources.presence_penalty, ParamSource::Layer(1));
    assert_eq!(
        discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG),
        vec!["presence_penalty"],
        "rung 0 asked for 1.2 and rung 1 won with 0.4 — rung 0 lost"
    );
    assert!(
        discarded_from_rung(&profile, &resolved, &sources, 1).is_empty(),
        "the same resolution, from the rung that won — nothing lost"
    );
}
