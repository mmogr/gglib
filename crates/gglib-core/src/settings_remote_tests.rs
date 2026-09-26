//! Tests for the remote tunnel's settings (ADR 0012) and what a reset keeps.
//!
//! Split out via `#[path]`, like `settings_tests.rs`, and separately from it
//! because that file is at its budget.

use super::*;

/// A pairing as `gglib remote join` leaves it.
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
/// Blank is worth refusing on the ticket too, not only on the key. `join`
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

/// A pairing written by `join` survives the round trip through the
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

/// The five fields a reset keeps, as the stored record names them.
const KEPT: [&str; 5] = [
    "proxy_api_key",
    "remote_pairing",
    "remote_enabled",
    "remote_serve",
    "remote_devices",
];

/// A machine that joined another, admits one device, serves remote access
/// with MCP on, holds a proxy key, and holds a value other than its default
/// in every other field.
///
/// The literal names every field, so a field added to `Settings` does not
/// compile here until it is given a value.
fn a_remote_machine() -> Settings {
    let sampling = InferenceConfig {
        temperature: Some(0.2),
        ..InferenceConfig::default()
    };
    Settings {
        proxy_api_key: Some("proxy-key".to_owned()),
        remote_pairing: Some(pairing()),
        remote_enabled: Some(true),
        remote_serve: Some(RemoteServe {
            allow_mcp: true,
            relay: Some("https://relay.example".to_owned()),
            discovery: false,
        }),
        remote_devices: Some(vec![Device {
            id: "dev-0a1b2c3d".to_owned(),
            label: Some("phone".to_owned()),
            joined_at: 1,
            redeemed_at: Some(2),
            last_seen: Some(3),
            peer: None,
        }]),
        default_download_path: Some("/models/elsewhere".to_owned()),
        default_context_size: Some(4096),
        proxy_port: Some(9191),
        llama_base_port: Some(9500),
        max_download_queue_size: Some(3),
        show_memory_fit_indicators: Some(false),
        max_tool_iterations: Some(40),
        max_stagnation_steps: Some(9),
        default_model_id: Some(7),
        inference_defaults: Some(sampling.clone()),
        inference_profiles: Some(vec![InferenceProfile {
            name: "focused".to_owned(),
            description: None,
            config: sampling,
            list_in_models: true,
        }]),
        setup_completed: Some(true),
        title_generation_prompt: Some("Name this chat.".to_owned()),
        bind_host: Some("0.0.0.0".to_owned()),
        share_lan: Some(true),
        trust_client_sampling: Some(true),
        loop_guard_mode: Some(LoopGuardMode::Refuse),
        proxy_loop_detection: Some(false),
        tool_call_repair: Some(false),
        agentic_sampling: Some(true),
        proxy_autostart: Some(true),
        close_to_tray: Some(true),
        start_at_login: Some(true),
    }
}

#[test]
fn a_reset_keeps_the_pairing_the_roster_and_the_remote_switch_and_flags() {
    let before = a_remote_machine();
    let mut after = a_remote_machine();

    after.reset_preferences();

    assert_eq!(after.remote_pairing, before.remote_pairing);
    assert_eq!(after.remote_devices, before.remote_devices);
    assert_eq!(after.remote_enabled, Some(true));
    assert_eq!(after.remote_serve, before.remote_serve);
}

#[test]
fn a_reset_keeps_the_proxy_api_key() {
    let mut after = a_remote_machine();

    after.reset_preferences();

    assert_eq!(after.proxy_api_key.as_deref(), Some("proxy-key"));
}

/// Everything that is not one of the five kept fields comes back as
/// `with_defaults` holds it.
///
/// The loop holds the fixture to its word: every field outside the five
/// differs from its default, so a reset that also kept any of them fails the
/// comparison below. A field given its default in the fixture fails the loop.
#[test]
fn a_reset_still_puts_every_preference_back_to_its_default() {
    let before = a_remote_machine();
    let stored = serde_json::to_value(&before).unwrap();
    let defaults = serde_json::to_value(Settings::with_defaults()).unwrap();
    let fields = stored.as_object().expect("settings serialize as a map");
    for (field, value) in fields.iter().filter(|(f, _)| !KEPT.contains(&f.as_str())) {
        assert_ne!(
            Some(value),
            defaults.get(field),
            "the fixture holds `{field}` at its default"
        );
    }
    let mut after = a_remote_machine();

    after.reset_preferences();

    assert_eq!(
        after,
        Settings {
            proxy_api_key: before.proxy_api_key,
            remote_pairing: before.remote_pairing,
            remote_enabled: before.remote_enabled,
            remote_serve: before.remote_serve,
            remote_devices: before.remote_devices,
            ..Settings::with_defaults()
        }
    );
}
