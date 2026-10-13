//! Tests for the `agentic_prompt_note` setting: the proxy's batching note
//! for a request that carries tools.

use serde_json::json;

use super::*;

/// The note is off when nothing is stored, and on only when stored on.
#[test]
fn the_prompt_note_is_off_unless_it_is_stored_on() {
    let mut settings = Settings::with_defaults();
    assert_eq!(settings.agentic_prompt_note, None);
    assert!(!settings.effective_agentic_prompt_note());
    assert!(!Settings::default().effective_agentic_prompt_note());

    settings.agentic_prompt_note = Some(false);
    assert!(!settings.effective_agentic_prompt_note());
    settings.agentic_prompt_note = Some(true);
    assert!(settings.effective_agentic_prompt_note());
}

/// The agentic-sampling switch stored off does not turn the note off: the
/// note is not a sampling change.
#[test]
fn the_agentic_sampling_switch_does_not_turn_the_note_off() {
    let settings = Settings {
        agentic_sampling: Some(false),
        agentic_prompt_note: Some(true),
        ..Settings::with_defaults()
    };

    assert!(settings.effective_agentic_prompt_note());
}

/// A stored record from before the setting existed reads it as unset.
#[test]
fn a_record_without_the_key_reads_it_as_unset() {
    let settings: Settings = serde_json::from_value(json!({"proxy_port": 9191})).unwrap();

    assert_eq!(settings.agentic_prompt_note, None);
    assert!(!settings.effective_agentic_prompt_note());
}

/// It is stored under its own name and read back as written.
#[test]
fn it_round_trips_through_the_stored_form() {
    let settings = Settings {
        agentic_prompt_note: Some(true),
        ..Settings::default()
    };

    let stored = serde_json::to_value(&settings).unwrap();
    assert_eq!(stored["agentic_prompt_note"], json!(true));
    let read: Settings = serde_json::from_value(stored).unwrap();
    assert_eq!(read, settings);
}

/// The merge writes the field when the update names it, clears it on
/// `Some(None)`, leaves it alone on `None`, and touches no neighbour.
#[test]
fn the_merge_sets_clears_and_leaves_the_prompt_note() {
    let mut settings = Settings {
        agentic_sampling: Some(false),
        ..Settings::default()
    };

    settings.merge(&SettingsUpdate {
        agentic_prompt_note: Some(Some(true)),
        ..Default::default()
    });
    assert_eq!(settings.agentic_prompt_note, Some(true));
    assert_eq!(settings.agentic_sampling, Some(false));
    assert_eq!(settings.agentic_thinking, None);

    settings.merge(&SettingsUpdate::default());
    assert_eq!(settings.agentic_prompt_note, Some(true));

    settings.merge(&SettingsUpdate {
        agentic_prompt_note: Some(Some(false)),
        ..Default::default()
    });
    assert_eq!(settings.agentic_prompt_note, Some(false));

    settings.merge(&SettingsUpdate {
        agentic_prompt_note: Some(None),
        ..Default::default()
    });
    assert_eq!(settings.agentic_prompt_note, None);
    assert_eq!(settings.agentic_sampling, Some(false));
}
