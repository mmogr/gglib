//! Tests for how a row's `defaults_origin` is resolved.

use super::*;

#[test]
fn stored_value_wins_when_present() {
    let origin = resolve_defaults_origin(
        Some("user".to_owned()),
        Some(&InferenceConfig::reasoning_profile()),
    );
    assert_eq!(
        origin,
        Some(DefaultsOrigin::User),
        "explicit column value must not be second-guessed, even though \
         this inference_defaults matches the auto-detected recipe \
         exactly — a user is free to set the same values by hand"
    );
}

#[test]
fn legacy_row_matching_the_reasoning_recipe_backfills_to_auto_detected() {
    let origin = resolve_defaults_origin(None, Some(&InferenceConfig::reasoning_profile()));
    assert_eq!(origin, Some(DefaultsOrigin::AutoDetected));
}

/// The measured origin round-trips through the same TEXT column with no
/// schema change — `Display` writes `"measured"`, `FromStr` reads it, and
/// the legacy backfill never manufactures it: a `Measured` row is always
/// explicitly written by an apply, so an unlabelled row can only be a
/// guess or a person's work.
#[test]
fn a_measured_origin_round_trips_and_is_never_backfilled() {
    let origin = resolve_defaults_origin(
        Some(DefaultsOrigin::Measured.to_string()),
        Some(&InferenceConfig::reasoning_profile()),
    );
    assert_eq!(origin, Some(DefaultsOrigin::Measured));

    // A legacy NULL beside any recipe backfills to a guess or to user —
    // never to measured.
    let backfilled = resolve_defaults_origin(None, Some(&InferenceConfig::reasoning_profile()));
    assert_ne!(backfilled, Some(DefaultsOrigin::Measured));
}

#[test]
fn legacy_row_not_matching_the_reasoning_recipe_backfills_to_user() {
    let custom = InferenceConfig {
        temperature: Some(0.3),
        ..Default::default()
    };
    let origin = resolve_defaults_origin(None, Some(&custom));
    assert_eq!(origin, Some(DefaultsOrigin::User));
}

#[test]
fn no_inference_defaults_means_no_origin_regardless_of_the_column() {
    assert_eq!(resolve_defaults_origin(Some("user".to_owned()), None), None);
    assert_eq!(resolve_defaults_origin(None, None), None);
}

#[test]
fn unparseable_stored_value_falls_back_to_the_recipe_match() {
    // A column value from some future, unrecognised variant must not
    // panic or silently become `None` — it falls through to the same
    // backfill a legacy NULL would get.
    let origin = resolve_defaults_origin(
        Some("not_a_real_variant".to_owned()),
        Some(&InferenceConfig::reasoning_profile()),
    );
    assert_eq!(origin, Some(DefaultsOrigin::AutoDetected));
}
