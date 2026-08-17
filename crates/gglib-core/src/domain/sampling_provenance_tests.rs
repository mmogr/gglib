//! Tests for [`super::FieldSources`] and sampling provenance reporting.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;

use crate::domain::{InferenceConfig, ReasoningEffort};

/// Fields of [`InferenceConfig`] that deliberately carry no provenance.
///
/// `seed` is request-scoped: no rung ever names one, so an entry for it would
/// read `unset by design` on every model forever. The same list, for the same
/// reason, as `sampling_explain`'s `NO_PROVENANCE` — named rather than
/// subtracted silently, so a field that loses its provenance by *accident*
/// still fails the check below.
const NO_PROVENANCE: [&str; 1] = ["seed"];

/// **The forcing function.** Nothing in the type system makes a new
/// [`InferenceConfig`] field appear here.
///
/// `seed` is the proof and the warning: it was added to `InferenceConfig`,
/// resolved through the ladder and sent on the wire while [`FieldSources`]
/// stayed exactly as wide as it had been, and the omission was a decision
/// nobody was asked to make. Adding a field is a one-line change; adding it
/// *and* its provenance is two, and only the compiler notices the second when
/// something like this asks.
///
/// The comparison runs over `to_openai_json_patch`'s keys rather than over the
/// struct's Rust field names because that patch is what actually reaches
/// llama-server — a field with no provenance entry is precisely a value gglib
/// sends and cannot explain.
#[test]
fn every_field_gglib_sends_has_a_provenance_entry() {
    // Every field `Some`, so the patch carries all of them. A `..Default`
    // spread would silently shrink this test to whatever happened to be set.
    let everything = InferenceConfig {
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
        reasoning_effort: Some(ReasoningEffort::High),
        reasoning_budget_tokens: Some(4096),
    };

    let sent: Vec<String> = everything.to_openai_json_patch().keys().cloned().collect();
    let explained: Vec<&str> = default_sources().iter().map(|(name, _)| name).collect();

    for key in &sent {
        assert!(
            explained.contains(&key.as_str()) || NO_PROVENANCE.contains(&key.as_str()),
            "{key} reaches llama-server with no FieldSources entry — add one, or \
             add it to NO_PROVENANCE with the reason"
        );
    }
    for excluded in NO_PROVENANCE {
        assert!(
            sent.contains(&excluded.to_owned()),
            "{excluded} is no longer a field gglib sends; drop it from NO_PROVENANCE"
        );
    }
    // The other direction: a provenance entry naming a field the patch has no
    // key for would render a row nothing can supply a value to.
    for field in &explained {
        assert!(
            sent.contains(&(*field).to_owned()),
            "FieldSources explains {field}, which InferenceConfig does not send"
        );
    }
    assert_eq!(explained.len(), sent.len() - NO_PROVENANCE.len());
}

/// A `FieldSources` with every field the same, for tests that only care about
/// the field *set*. Deliberately a full literal: it is the second half of the
/// forcing function above, and a `..Default::default()` here would let a new
/// field default in silently.
fn default_sources() -> FieldSources {
    let s = ParamSource::Unset;
    FieldSources {
        temperature: s,
        top_p: s,
        top_k: s,
        presence_penalty: s,
        repeat_penalty: s,
        min_p: s,
        frequency_penalty: s,
        dynatemp_range: s,
        dynatemp_exponent: s,
        top_n_sigma: s,
        dry_multiplier: s,
        dry_base: s,
        dry_allowed_length: s,
        dry_penalty_last_n: s,
        max_tokens: s,
        reasoning_effort: s,
        reasoning_budget_tokens: s,
    }
}

#[test]
fn iter_yields_every_field_once_in_display_order() {
    let sources = FieldSources {
        temperature: ParamSource::Layer(0),
        top_p: ParamSource::Layer(0),
        top_k: ParamSource::Floor,
        presence_penalty: ParamSource::FloorCoupled,
        repeat_penalty: ParamSource::FloorCoupled,
        min_p: ParamSource::FloorCoupled,
        dynatemp_range: ParamSource::Unset,
        dynatemp_exponent: ParamSource::Unset,
        top_n_sigma: ParamSource::Unset,
        dry_multiplier: ParamSource::FloorCoupled,
        dry_base: ParamSource::Unset,
        dry_allowed_length: ParamSource::Unset,
        dry_penalty_last_n: ParamSource::Unset,
        frequency_penalty: ParamSource::Unset,
        max_tokens: ParamSource::Unset,
        reasoning_effort: ParamSource::Unset,
        reasoning_budget_tokens: ParamSource::Unset,
    };
    let fields: Vec<&str> = sources.iter().map(|(name, _)| name).collect();
    assert_eq!(
        fields,
        [
            "temperature",
            "top_p",
            "top_k",
            "presence_penalty",
            "repeat_penalty",
            "min_p",
            "frequency_penalty",
            "dynatemp_range",
            "dynatemp_exponent",
            "top_n_sigma",
            "dry_multiplier",
            "dry_base",
            "dry_allowed_length",
            "dry_penalty_last_n",
            "max_tokens",
            "reasoning_effort",
            "reasoning_budget_tokens",
        ]
    );
}

/// Both floor variants render as `floor` in the terse log form — the
/// distinction exists for the `explain` command, which has room for it.
#[test]
fn describe_names_layers_and_collapses_the_floor_variants() {
    let sources = FieldSources {
        temperature: ParamSource::Layer(1),
        top_p: ParamSource::Layer(0),
        top_k: ParamSource::Floor,
        presence_penalty: ParamSource::FloorCoupled,
        repeat_penalty: ParamSource::Layer(2),
        min_p: ParamSource::Floor,
        frequency_penalty: ParamSource::Unset,
        dynatemp_range: ParamSource::Unset,
        dynatemp_exponent: ParamSource::Unset,
        top_n_sigma: ParamSource::Unset,
        dry_multiplier: ParamSource::Layer(2),
        dry_base: ParamSource::Unset,
        dry_allowed_length: ParamSource::Unset,
        dry_penalty_last_n: ParamSource::Unset,
        max_tokens: ParamSource::Unset,
        reasoning_effort: ParamSource::Unset,
        reasoning_budget_tokens: ParamSource::Unset,
    };
    let got = sources.describe(&["cli", "profile", "model"]);
    assert!(got.contains("temperature=profile"), "{got}");
    assert!(got.contains("top_p=cli"), "{got}");
    assert!(got.contains("top_k=floor"), "{got}");
    assert!(got.contains("presence_penalty=floor"), "{got}");
    assert!(got.contains("repeat_penalty=model"), "{got}");
    assert!(got.contains("dry_multiplier=model"), "{got}");
    assert!(got.contains("dry_base=unset"), "{got}");
    assert!(got.contains("max_tokens=unset"), "{got}");
}

/// A names array that does not cover the ladder is a caller bug; render it
/// visibly rather than panicking inside a log line.
#[test]
fn describe_marks_an_index_the_names_do_not_cover() {
    let sources = FieldSources {
        temperature: ParamSource::Layer(9),
        top_p: ParamSource::Floor,
        top_k: ParamSource::Floor,
        presence_penalty: ParamSource::Floor,
        repeat_penalty: ParamSource::Floor,
        min_p: ParamSource::Floor,
        frequency_penalty: ParamSource::Unset,
        dynatemp_range: ParamSource::Unset,
        dynatemp_exponent: ParamSource::Unset,
        top_n_sigma: ParamSource::Unset,
        dry_multiplier: ParamSource::Floor,
        dry_base: ParamSource::Unset,
        dry_allowed_length: ParamSource::Unset,
        dry_penalty_last_n: ParamSource::Unset,
        max_tokens: ParamSource::Unset,
        reasoning_effort: ParamSource::Unset,
        reasoning_budget_tokens: ParamSource::Unset,
    };
    assert!(sources.describe(&["cli"]).contains("temperature=?"));
}

#[test]
fn layer_indices_match_the_resolve_with_profile_ladder() {
    assert_eq!(SamplingLayer::from_index(0), Some(SamplingLayer::Request));
    assert_eq!(SamplingLayer::from_index(1), Some(SamplingLayer::Profile));
    assert_eq!(
        SamplingLayer::from_index(2),
        Some(SamplingLayer::ModelUserSet)
    );
    assert_eq!(SamplingLayer::from_index(3), Some(SamplingLayer::Global));
    assert_eq!(
        SamplingLayer::from_index(4),
        Some(SamplingLayer::ModelAutoDetected)
    );
    assert_eq!(SamplingLayer::from_index(5), None);
}
