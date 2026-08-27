//! Unit tests for [`super`].

use super::*;

#[test]
fn kebab_round_trips_through_camel() {
    for (kebab, camel) in [
        ("default-context-size", "defaultContextSize"),
        ("proxy-port", "proxyPort"),
        ("share-lan", "shareLan"),
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
    assert!(keys.contains_key("proxyLoopDetection"));
    for (k, v) in &keys {
        assert_eq!(v, &Value::Null, "{k} should serialise as null when unset");
    }
}

/// The motivating case. `settings set` can only write a value, so before this
/// existed a stored 4096 outranked the fitted rung with no way back short of
/// resetting everything.
#[test]
fn a_null_for_a_known_key_reads_as_clear_not_as_absent() {
    let body = Value::Object(
        [("defaultContextSize".to_string(), Value::Null)]
            .into_iter()
            .collect(),
    );
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

/// Every key `settings show` prints must be one `settings unset` accepts.
/// The two surfaces disagreeing is the defect this subcommand exists to fix,
/// one level up.
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
