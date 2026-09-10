//! Tests for the remote tunnel's stored pairing (ADR 0012).
//!
//! Split out via `#[path]`, like `settings_tests.rs`, and separately from it
//! because that file is at its budget.

use super::*;

/// A pairing as `gglib remote connect` leaves it.
fn pairing() -> RemotePairing {
    RemotePairing {
        ticket: "pipeabc".to_owned(),
        api_key: "k".to_owned(),
        default_model: None,
        port: None,
    }
}

/// Each half of the stored pairing follows the proxy key's rule: a blank is
/// refused, and clearing the whole record is how the pairing is forgotten.
///
/// Blank is worth refusing on the ticket too, not only on the key. `connect`
/// reads the ticket as the address to dial and the name of the machine whose
/// key it holds, so a blank one would dial nothing while still claiming this
/// machine is paired with something.
#[test]
fn a_blank_half_of_a_pairing_is_refused_and_a_cleared_record_is_fine() {
    let blank_key = Settings {
        remote_pairing: Some(RemotePairing {
            api_key: "  ".to_owned(),
            default_model: None,
            port: None,
            ..pairing()
        }),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&blank_key),
        Err(SettingsError::BlankRemoteApiKey)
    ));

    let blank_ticket = Settings {
        remote_pairing: Some(RemotePairing {
            ticket: String::new(),
            ..pairing()
        }),
        ..Default::default()
    };
    assert!(matches!(
        validate_settings(&blank_ticket),
        Err(SettingsError::BlankRemoteTicket)
    ));

    let mut settings = Settings {
        remote_pairing: Some(pairing()),
        ..Default::default()
    };
    assert!(validate_settings(&settings).is_ok());
    settings.merge(&SettingsUpdate {
        remote_pairing: Some(None),
        ..SettingsUpdate::default()
    });
    assert_eq!(settings.remote_pairing, None);
    assert!(validate_settings(&settings).is_ok());
}

/// A pairing written by `connect` survives the round trip through the
/// key-value store's JSON as one row, and a database that has never paired
/// loads as "never paired".
#[test]
fn the_stored_pairing_round_trips_as_one_value_and_defaults_absent() {
    let settings = Settings {
        remote_pairing: Some(pairing()),
        ..Default::default()
    };
    let json = serde_json::to_string(&settings).unwrap();
    let back: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.remote_pairing, Some(pairing()));

    let old: Settings = serde_json::from_str(r#"{"proxy_port":8080}"#).unwrap();
    assert_eq!(old.remote_pairing, None);
}

/// The rows a build before the binding wrote are **ignored**, not folded in.
///
/// The two carried no evidence about each other — that is the whole defect —
/// so reading them as a pairing would restore exactly the state this field
/// exists to make unrepresentable: a key that belongs to whichever machine
/// happens to be named beside it. A machine upgrading over such a database
/// loads as never paired and pairs again, which the ticket's own staleness
/// already asked of it.
///
/// An alias would be the reflex, and is a trap here rather than a shortcut:
/// serde treats an alias as the same field, so the first save after the
/// upgrade — which writes `remote_pairing` and cannot delete a row no
/// current field names — leaves a database whose next load fails outright
/// with `duplicate field`.
#[test]
fn the_rows_written_before_the_pairing_was_one_record_are_ignored() {
    let legacy: Settings = serde_json::from_str(
        r#"{"proxy_port":8080,"remote_api_key":"machine-a-key","remote_last_ticket":"pipeb"}"#,
    )
    .expect("a settings row this build no longer names must not fail the load");

    assert_eq!(legacy.remote_pairing, None);
    assert_eq!(legacy.proxy_port, Some(8080));
    assert!(
        !serde_json::to_string(&legacy)
            .unwrap()
            .contains("machine-a-key"),
        "and nothing carries the orphaned key back out"
    );
}

/// A record written before `defaultModel` existed loads as nothing
/// remembered yet, not as a record this build cannot read — a pairing is
/// expensive to replace and a missing field is not a reason to.
#[test]
fn a_pairing_stored_before_the_remembered_model_still_loads() {
    let row = r#"{"ticket":"pipeabc","apiKey":"k"}"#;
    let loaded: RemotePairing = serde_json::from_str(row).expect("an older record loads");
    assert_eq!(loaded.default_model, None);
    assert_eq!(loaded.port, None);
    let with = RemotePairing {
        default_model: Some("qwen3".to_owned()),
        port: None,
        ..pairing()
    };
    let round: RemotePairing =
        serde_json::from_str(&serde_json::to_string(&with).unwrap()).unwrap();
    assert_eq!(round.default_model.as_deref(), Some("qwen3"));
}
