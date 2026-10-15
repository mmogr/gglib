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

/// The case this module was written for, now resolved the other way: the
/// profile claims the temperature beneath the request, and the coupling rule
/// only passes over rungs beneath the claim, so the request's penalty stands
/// and there is nothing to report (ADR 0022).
#[test]
fn a_coupled_flag_above_a_profiles_claim_stands() {
    let request = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let profile = InferenceConfig {
        temperature: Some(0.8),
        ..Default::default()
    };
    let (resolved, sources) = resolve(&request, Some(&profile), None);

    assert_eq!(sources.presence_penalty, ParamSource::Layer(REQUEST_RUNG));
    assert_eq!(resolved.presence_penalty, Some(1.2));
    assert!(discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG).is_empty());
}

/// Nor does a claimant that names its own value beneath the request override
/// the request's: the higher rung wins, as for any field.
#[test]
fn a_coupled_flag_beats_the_claiming_layers_own() {
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

    assert_eq!(sources.presence_penalty, ParamSource::Layer(REQUEST_RUNG));
    assert_eq!(resolved.presence_penalty, Some(1.2));
    assert!(discarded_from_rung(&request, &resolved, &sources, REQUEST_RUNG).is_empty());
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
/// rungs, must give two answers. The pipeline's ladder puts the operator's
/// flags at rung 0 and the client at 1; a helper that hardcoded rung 0 would
/// report the client's losses as the operator's.
#[test]
fn the_rung_index_is_respected() {
    let cli = InferenceConfig {
        presence_penalty: Some(0.4),
        ..Default::default()
    };
    let client = InferenceConfig {
        presence_penalty: Some(1.2),
        ..Default::default()
    };
    let (resolved, sources) =
        InferenceConfig::resolve_layers_with_sources(&[Some(&cli), Some(&client)]);

    assert_eq!(sources.presence_penalty, ParamSource::Layer(0));
    assert_eq!(
        discarded_from_rung(&client, &resolved, &sources, 1),
        vec!["presence_penalty"],
        "rung 1 asked for 1.2 and rung 0 won with 0.4 — rung 1 lost"
    );
    assert!(
        discarded_from_rung(&cli, &resolved, &sources, 0).is_empty(),
        "the same resolution, from the rung that won — nothing lost"
    );
}
