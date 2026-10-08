//! Unit tests for [`super`].

use super::*;
use crate::bootstrap::test_context;

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
    assert!(keys.contains_key("loopGuardMode"));
    for (k, v) in &keys {
        assert_eq!(v, &Value::Null, "{k} should serialise as null when unset");
    }
}

/// `proxy-loop-detection` named the loop guard's switch before
/// `loop-guard-mode`, and is not a setting now: it is refused as any name the
/// wire type does not carry is, and nothing is stored.
#[tokio::test]
async fn the_retired_loop_detection_key_is_refused_as_unknown() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let before = ctx.app.settings().get().await.expect("settings");

    let refused = handle_unset(&ctx, "proxy-loop-detection")
        .await
        .expect_err("not a setting");

    let said = format!("{refused:#}");
    assert!(
        said.starts_with("Unknown setting 'proxy-loop-detection'."),
        "{said}"
    );
    assert!(said.contains("\n  loop-guard-mode\n"), "{said}");
    assert_eq!(ctx.app.settings().get().await.expect("settings"), before);
}

/// The motivating case. `settings set` can only write a value, so before this
/// existed a stored 4096 outranked the fitted rung with no way back short of
/// resetting everything.
#[test]
#[allow(
    clippy::iter_on_single_items,
    reason = "grandfathered at lint inheritance, #1157"
)]
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
