//! Tests for the settings display rows.
//!
//! Split out via `#[path]`, the house answer to a file at its size budget:
//! `settings_display.rs` sits over the ratchet's line and may shrink but not
//! grow, and the masking rule the stored pairing needed had to be written
//! somewhere.

use gglib_core::domain::InferenceConfig;
use gglib_core::{RemotePairing, Settings};

use super::{MASKED_VALUE, camel_to_kebab, settings_display_rows};

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
    let rows = settings_display_rows(&settings, None);
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
    let rows = settings_display_rows(&settings, None);
    let (_, value) = rows
        .iter()
        .find(|(k, _)| k == "default-context-size")
        .expect("the row is listed");
    assert_eq!(value, "32768");
}

#[test]
fn settings_display_rows_uses_kebab_case_keys() {
    let settings = Settings::default();
    let rows = settings_display_rows(&settings, None);

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
    let rows = settings_display_rows(&settings, None);
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
    let rows = settings_display_rows(&settings, None);

    for expected in ["proxy-autostart", "close-to-tray", "start-at-login"] {
        assert!(
            rows.iter().any(|(k, _)| k == expected),
            "{expected} must appear in display rows"
        );
    }
}

/// The proxy API key is shown in full, deliberately.
///
/// Masking it would be the reflex, but the proxy *generates* this value
/// when it binds a non-loopback host, and `settings show` is then the only
/// way to recover it — a masked row would strand someone who lost the
/// startup banner. It is a local credential in a local database that the
/// reader can already read; hiding it buys nothing and costs recovery.
#[test]
fn the_proxy_api_key_is_shown_rather_than_masked() {
    let settings = Settings {
        proxy_api_key: Some("secret123".to_owned()),
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, None);

    assert_eq!(
        rows.iter()
            .find(|(k, _)| k == "proxy-api-key")
            .map(|(_, v)| v.as_str()),
        Some("secret123"),
        "the key must be recoverable from `settings show`: {rows:?}"
    );
}

/// The received remote key is not, for the opposite reason — and it is
/// masked *inside* the pairing record, one level down.
///
/// It belongs to the other machine and arrives over the wire; re-pairing
/// replaces it, so nothing needs to read it back. The field's own doc said
/// no settings surface exposed it while this one printed it in full. The
/// depth is the part worth a test: masking was a check on the top-level
/// key alone, and the day the key moved inside the pairing that check
/// stopped matching anything while still reporting success.
#[test]
fn the_received_remote_key_is_masked_inside_the_pairing_record() {
    let settings = Settings {
        remote_pairing: Some(RemotePairing {
            ticket: "ticket-abc".to_owned(),
            api_key: "the-other-machines-key".to_owned(),
        }),
        ..Default::default()
    };
    let rows = settings_display_rows(&settings, None);
    let value = |key: &str| {
        rows.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .unwrap_or_default()
            .to_owned()
    };

    assert_eq!(value("remote-pairing.api-key"), MASKED_VALUE, "{rows:?}");
    assert!(
        !rows
            .iter()
            .any(|(_, v)| v.contains("the-other-machines-key")),
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
    let rows = settings_display_rows(&Settings::default(), None);

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
    let rows = settings_display_rows(&settings, Some("42 (TestModel)".to_owned()));
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
    let rows = settings_display_rows(&settings, None);
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
    let rows = settings_display_rows(&settings, None);
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
    let rows = settings_display_rows(&settings, None);

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
