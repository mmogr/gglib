//! Tests for how a row's `defaults_origin` is resolved.

use super::*;

#[test]
fn stored_value_wins_when_present() {
    let origin = resolve_defaults_origin(
        Some(DefaultsOrigin::Measured.to_string()),
        Some(&InferenceConfig::reasoning_profile()),
    );
    assert_eq!(origin, Some(DefaultsOrigin::Measured));
}

/// No comparison against a recipe: a person may set the recipe's own values
/// by hand, and gglib labels everything it writes itself.
#[test]
fn an_unlabelled_row_is_a_persons() {
    let origin = resolve_defaults_origin(None, Some(&InferenceConfig::reasoning_profile()));
    assert_eq!(origin, Some(DefaultsOrigin::User));
}

#[test]
fn no_inference_defaults_means_no_origin_regardless_of_the_column() {
    assert_eq!(resolve_defaults_origin(Some("user".to_owned()), None), None);
    assert_eq!(resolve_defaults_origin(None, None), None);
}

/// A label this build does not know — `auto_detected` among them, which no
/// build writes any more — must not panic or vanish; it reads as a person's.
#[test]
fn an_unknown_label_reads_as_a_persons() {
    for label in ["auto_detected", "not_a_real_variant"] {
        let origin = resolve_defaults_origin(
            Some(label.to_owned()),
            Some(&InferenceConfig::reasoning_profile()),
        );
        assert_eq!(origin, Some(DefaultsOrigin::User), "{label}");
    }
}
