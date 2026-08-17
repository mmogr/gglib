//! Tests for [`super::FieldSources`] and sampling provenance reporting.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;

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
