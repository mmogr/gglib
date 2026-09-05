//! Tests for the remote tunnel's two settings fields (ADR 0012).
//!
//! Split out via `#[path]`, like `settings_tests.rs`, and separately from it
//! because that file is at its budget.

use super::*;

/// The connect side's stored pairing follows the proxy key's rule: a blank is
/// refused, a cleared field is how the pairing is forgotten.
#[test]
fn a_blank_remote_credential_is_refused_and_a_cleared_one_is_fine() {
    let blank_key = Settings {
        remote_api_key: Some("  ".to_owned()),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&blank_key),
        Err(SettingsError::BlankRemoteApiKey)
    ));

    let blank_ticket = Settings {
        remote_last_ticket: Some(String::new()),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&blank_ticket),
        Err(SettingsError::BlankRemoteTicket)
    ));

    let mut settings = Settings {
        remote_api_key: Some("k".to_owned()),
        remote_last_ticket: Some("pipeabc".to_owned()),
        ..Default::default()
    };
    assert!(validate_settings(&settings).is_ok());
    settings.merge(&SettingsUpdate {
        remote_api_key: Some(None),
        remote_last_ticket: Some(None),
        ..SettingsUpdate::default()
    });
    assert_eq!(settings.remote_api_key, None);
    assert_eq!(settings.remote_last_ticket, None);
    assert!(validate_settings(&settings).is_ok());
}

/// A pairing written by `connect` survives the round trip through the
/// key-value store's JSON, and an older database without the rows loads as
/// "never paired".
#[test]
fn the_remote_pairing_round_trips_and_defaults_absent() {
    let settings = Settings {
        remote_api_key: Some("key".to_owned()),
        remote_last_ticket: Some("pipeabc".to_owned()),
        ..Default::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let back: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.remote_api_key.as_deref(), Some("key"));
    assert_eq!(back.remote_last_ticket.as_deref(), Some("pipeabc"));

    let old: Settings = serde_json::from_str(r#"{"proxy_port":8080}"#).unwrap();
    assert_eq!(old.remote_api_key, None);
    assert_eq!(old.remote_last_ticket, None);
}
