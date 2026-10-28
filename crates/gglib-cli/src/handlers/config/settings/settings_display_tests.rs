//! Tests for the settings display rows.
//!
//! Split out via `#[path]`, the house answer to a file at its size budget:
//! `settings_display.rs` sits over the ratchet's line and may shrink but not
//! grow, and the masking rule the stored pairing needed had to be written
//! somewhere.

use gglib_core::domain::InferenceConfig;
use gglib_core::secret::SHOWN_AS;
use gglib_core::{RemotePairing, Settings};

use super::{camel_to_kebab, settings_display_rows};

// ── camel_to_kebab ────────────────────────────────────────────────────────

#[test]
fn camel_to_kebab_converts_correctly() {
    assert_eq!(camel_to_kebab("topK"), "top-k");
    assert_eq!(camel_to_kebab("maxTokens"), "max-tokens");
    assert_eq!(camel_to_kebab("repeatPenalty"), "repeat-penalty");
    assert_eq!(camel_to_kebab("temperature"), "temperature");
    assert_eq!(camel_to_kebab("topP"), "top-p");
}

// ── settings_display_rows ─────────────────────────────────────────────────

/// An unset context is the recommended state, not a gap, and `show` is
/// where a person looks to find out what is configured. A bare "None"
/// there reads as something missing.
#[test]
fn an_unset_context_size_says_what_unset_means() {
    let settings = Settings {
        default_context_size: None,
        ..Settings::default()
    };
    let rows = settings_display_rows(&settings, None, None);
    let (_, value) = rows
        .iter()
        .find(|(k, _)| k == "default-context-size")
        .expect("the row is listed");
    assert_eq!(value, "None (sized per launch)");
}

/// A number the user chose is shown as that number, with nothing added.
#[test]
fn a_chosen_context_size_is_shown_plainly() {
    let settings = Settings {
        default_context_size: Some(32_768),
        ..Settings::default()
    };
    let rows = settings_display_rows(&settings, None, None);
    let (_, value) = rows
        .iter()
        .find(|(k, _)| k == "default-context-size")
        .expect("the row is listed");
    assert_eq!(value, "32768");
}

#[test]
fn settings_display_rows_uses_kebab_case_keys() {
    let settings = Settings::default();
    let rows = settings_display_rows(&settings, None, None);

    assert!(!rows.is_empty(), "should produce at least one row");

    for (key, _) in &rows {
        assert!(
            key.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.'),
            "key {key:?} must contain only [a-z0-9-.] characters"
        );
        assert!(
            !key.contains('_'),
            "key {key:?} must not contain underscores"
        );
    }

    // No duplicate keys.
    let mut seen = std::collections::BTreeSet::new();
    for (key, _) in &rows {
        assert!(seen.insert(key.clone()), "duplicate key {key:?}");
    }
}

#[test]
fn setup_completed_is_hidden() {
    let settings = Settings::default();
    let rows = settings_display_rows(&settings, None, None);
    assert!(
        rows.iter().all(|(k, _)| k != "setup-completed"),
        "setup-completed must not appear in display rows"
    );
}

/// The always-on proxy settings are configurable from the GUI and the Web
/// UI, so `settings show` has to list them too — otherwise the CLI is the
/// one interface that cannot tell you why the proxy came up on its own.
#[test]
fn always_on_proxy_settings_are_listed() {
    let settings = Settings::default();
    let rows = settings_display_rows(&settings, None, None);

    for expected in ["proxy-autostart", "close-to-tray", "start-at-login"] {
        assert!(
            rows.iter().any(|(k, _)| k == expected),
            "{expected} must appear in display rows"
        );
    }
}

/// The proxy key is held, not printed, and its row names the command that
/// prints it. `settings set` echoes through these rows, so it does not print
/// the key it was just given either (ADR 0027).
#[test]
fn the_proxy_api_key_is_masked_and_names_its_reveal() {
    let settings = Settings {
        proxy_api_key: Some("secret123".to_owned()),
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, None, None);

    assert_eq!(
        rows.iter()
            .find(|(k, _)| k == "proxy-api-key")
            .map(|(_, v)| v.as_str()),
        Some(
            format!("{SHOWN_AS}  `gglib config settings reveal proxy-api-key` prints it").as_str()
        ),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|(_, v)| v.contains("secret123")),
        "the key must not reach any row: {rows:?}"
    );
}

/// A blank left by an older database is no key, as the proxy reads it, and
/// an absent one is "None": whether a key is held is not the secret.
#[test]
fn an_absent_or_blank_proxy_api_key_reads_as_none() {
    for stored in [None, Some("   ")] {
        let settings = Settings {
            proxy_api_key: stored.map(str::to_owned),
            ..Default::default()
        };
        let rows = settings_display_rows(&settings, None, None);
        let row = rows.iter().find(|(k, _)| k == "proxy-api-key");

        assert_eq!(row.map(|(_, v)| v.as_str()), Some("None"), "{stored:?}");
    }
}

/// The received remote key is masked too, *inside* the pairing record, one
/// level down: a check on the top-level key alone would match nothing there
/// and still report success.
#[test]
fn the_received_remote_key_is_masked_inside_the_pairing_record() {
    let settings = Settings {
        remote_pairing: Some(RemotePairing {
            ticket: "ticket-abc".to_owned(),
            api_key: "this-devices-key".to_owned(),
            default_model: None,
            port: None,
            name: None,
        }),
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, None, None);
    let value = |key: &str| {
        rows.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };

    assert_eq!(
        value("remote-pairing.api-key"),
        format!("{SHOWN_AS}  `gglib config settings reveal remote-pairing.api-key` prints it"),
        "{rows:?}"
    );
    assert!(
        !rows.iter().any(|(_, v)| v.contains("this-devices-key")),
        "the key must not reach any row: {rows:?}"
    );
    assert_eq!(
        value("remote-pairing.ticket"),
        "ticket-abc",
        "the ticket is an address, not a credential, and stays readable"
    );
}

/// Masking must not hide *whether* a pairing is held — that is what a
/// person runs this command to find out, and it is not the secret.
#[test]
fn an_absent_pairing_still_reads_as_none() {
    let rows = settings_display_rows(&Settings::default(), None, None);

    assert_eq!(
        rows.iter()
            .find(|(k, _)| k == "remote-pairing")
            .map(|(_, v)| v.as_str()),
        Some("None"),
        "unset must stay distinguishable from set: {rows:?}"
    );
}

#[test]
fn settings_display_rows_model_display_override() {
    let settings = Settings {
        default_model_id: Some(42),
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, Some("42 (TestModel)".to_owned()), None);
    let model_row = rows
        .iter()
        .find(|(k, _)| k == "default-model-id")
        .expect("default-model-id row should be present");
    assert_eq!(model_row.1, "42 (TestModel)");
}

#[test]
fn settings_display_rows_null_displays_as_none() {
    let settings = Settings {
        default_download_path: None,
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, None, None);
    let row = rows
        .iter()
        .find(|(k, _)| k == "default-download-path")
        .expect("default-download-path should be present");
    assert_eq!(row.1, "None");
}

#[test]
fn inference_defaults_null_emits_bare_none_row() {
    let settings = Settings {
        inference_defaults: None,
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, None, None);
    let row = rows
        .iter()
        .find(|(k, _)| k == "inference-defaults")
        .expect("bare inference-defaults row should be present when field is null");
    assert_eq!(row.1, "None");
}

#[test]
fn inference_defaults_expanded_to_sub_rows() {
    let settings = Settings {
        inference_defaults: Some(InferenceConfig {
            temperature: Some(0.75),
            top_k: Some(20),
            ..Default::default()
        }),
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, None, None);

    // No bare "inference-defaults" row when the field is set.
    assert!(
        rows.iter().all(|(k, _)| k != "inference-defaults"),
        "bare inference-defaults row must not appear when the field is set"
    );

    let temp = rows
        .iter()
        .find(|(k, _)| k == "inference-defaults.temperature")
        .expect("inference-defaults.temperature should be present");
    assert_eq!(temp.1, "0.75");

    let topk = rows
        .iter()
        .find(|(k, _)| k == "inference-defaults.top-k")
        .expect("inference-defaults.top-k should be present");
    assert_eq!(topk.1, "20");

    let maxtok = rows
        .iter()
        .find(|(k, _)| k == "inference-defaults.max-tokens")
        .expect("inference-defaults.max-tokens should be present");
    assert_eq!(maxtok.1, "None");
}
