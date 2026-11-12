//! Unit tests for the key names [`super`] builds its request under.

use gglib_app_services::types::UpdateSettingsRequest;
use serde_json::Value;

use super::super::settings_display::{camel_to_kebab, kebab_to_camel};
use super::super::wire::known_keys;

#[test]
fn kebab_round_trips_through_camel() {
    for (kebab, camel) in [
        ("default-context-size", "defaultContextSize"),
        ("proxy-port", "proxyPort"),
        ("proxy-host", "proxyHost"),
        ("setup-completed", "setupCompleted"),
    ] {
        assert_eq!(kebab_to_camel(kebab), camel);
        assert_eq!(camel_to_kebab(camel), kebab);
    }
}

/// The key list is read from the type, so it cannot drift from what `update`
/// accepts. This asserts the mechanism works at all — a `skip_serializing_if`
/// added to those fields later would silently empty the manifest and make every
/// key "unknown".
#[test]
fn the_wire_type_is_its_own_manifest() {
    let keys = known_keys();
    assert!(
        keys.len() > 20,
        "expected every settable field to appear, got {}: {keys:?}",
        keys.len()
    );
    assert!(keys.contains_key("defaultContextSize"));
    assert!(keys.contains_key("loopGuardMode"));
    for (k, v) in &keys {
        assert_eq!(v, &Value::Null, "{k} should serialise as null when unset");
    }
}

/// The motivating case. `set` can only write a value, so before this
/// existed a stored 4096 outranked the fitted rung with no way back short of
/// resetting everything.
#[test]
fn a_null_for_a_known_key_reads_as_clear_not_as_absent() {
    let body =
        Value::Object(std::iter::once(("defaultContextSize".to_string(), Value::Null)).collect());
    let req: UpdateSettingsRequest = serde_json::from_value(body).expect("known key");
    assert_eq!(
        req.default_context_size,
        Some(None),
        "an explicit null must clear, not leave alone"
    );
    assert_eq!(
        req.proxy_port, None,
        "an omitted key must leave that setting alone"
    );
}

/// Every key the wire type carries maps to a kebab-case key and back, so
/// the name `config show` prints is the name the request is built under.
#[test]
fn every_known_key_is_accepted_in_kebab_form() {
    for camel in known_keys().keys() {
        let kebab = camel_to_kebab(camel);
        assert_eq!(
            &kebab_to_camel(&kebab),
            camel,
            "{kebab} must map back to {camel}"
        );
    }
}
