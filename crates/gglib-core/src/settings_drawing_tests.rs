//! Tests for the two drawing settings: the default image model and the
//! `/mcp` drawing switch.

use serde_json::json;

use super::*;

/// The switch is off when nothing is stored, and on only when stored on.
#[test]
fn the_mcp_drawing_switch_is_off_when_absent() {
    let mut settings = Settings::with_defaults();
    assert_eq!(settings.mcp_drawing, None);
    assert!(!settings.effective_mcp_drawing());
    assert!(!Settings::default().effective_mcp_drawing());

    settings.mcp_drawing = Some(false);
    assert!(!settings.effective_mcp_drawing());
    settings.mcp_drawing = Some(true);
    assert!(settings.effective_mcp_drawing());
}

/// A stored record from before either setting existed reads both as unset.
#[test]
fn a_record_without_either_key_reads_both_as_unset() {
    let settings: Settings = serde_json::from_value(json!({"proxy_port": 9191})).unwrap();

    assert_eq!(settings.default_image_model_id, None);
    assert_eq!(settings.mcp_drawing, None);
    assert!(!settings.effective_mcp_drawing());
}

/// Both are stored under their own names and read back as written.
#[test]
fn both_round_trip_through_the_stored_form() {
    let settings = Settings {
        default_image_model_id: Some(7),
        mcp_drawing: Some(true),
        ..Settings::default()
    };

    let stored = serde_json::to_value(&settings).unwrap();
    assert_eq!(stored["default_image_model_id"], json!(7));
    assert_eq!(stored["mcp_drawing"], json!(true));
    let read: Settings = serde_json::from_value(stored).unwrap();
    assert_eq!(read, settings);
}

/// The merge writes each field when the update names it, clears it on
/// `Some(None)`, leaves it alone on `None`, and keeps the two apart from
/// `default_model_id`, whose twin the image model is.
#[test]
fn the_merge_sets_clears_and_leaves_each_drawing_setting() {
    let mut settings = Settings {
        default_model_id: Some(3),
        ..Settings::default()
    };

    settings.merge(&SettingsUpdate {
        default_image_model_id: Some(Some(7)),
        mcp_drawing: Some(Some(true)),
        ..Default::default()
    });
    assert_eq!(settings.default_image_model_id, Some(7));
    assert_eq!(settings.mcp_drawing, Some(true));
    assert_eq!(settings.default_model_id, Some(3));

    settings.merge(&SettingsUpdate::default());
    assert_eq!(settings.default_image_model_id, Some(7));
    assert_eq!(settings.mcp_drawing, Some(true));

    settings.merge(&SettingsUpdate {
        default_image_model_id: Some(None),
        mcp_drawing: Some(None),
        ..Default::default()
    });
    assert_eq!(settings.default_image_model_id, None);
    assert_eq!(settings.mcp_drawing, None);
    assert_eq!(settings.default_model_id, Some(3));
}
