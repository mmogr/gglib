//! Tests for [`super::TemplateCaps`] and friends.
//!
//! Split out via `#[path]` so the module itself stays inside the file budget.

use super::*;

/// The nine wire keys, byte-for-byte as the measurement transcribed them from
/// the pinned build (`b1-10bf611`), sorted. Transcribed from the wire, not
/// from the C++ source — the report is the contract, per ADR 0007's method.
const MEASURED_KEYS: [&str; 9] = [
    "supports_object_arguments",
    "supports_parallel_tool_calls",
    "supports_preserve_reasoning",
    "supports_reasoning_effort",
    "supports_string_content",
    "supports_system_role",
    "supports_tool_calls",
    "supports_tools",
    "supports_typed_content",
];

/// Shaped like config A (gpt-oss template) of the 2026-08-17 measurement:
/// the key set and the explicit-`false` serialization are transcribed from
/// the wire — the pinned build always writes all nine, `false`s included —
/// and `supports_reasoning_effort: true` is A's measured value. The other
/// bools are representative; the pin below is about the *keys*.
const MEASURED_CONFIG_A: &str = r#"{
    "supports_tools": true,
    "supports_tool_calls": true,
    "supports_system_role": true,
    "supports_parallel_tool_calls": true,
    "supports_preserve_reasoning": true,
    "supports_reasoning_effort": true,
    "supports_string_content": true,
    "supports_typed_content": false,
    "supports_object_arguments": false
}"#;

/// **The pin.** Serializing a fully-populated `TemplateCaps` must yield
/// exactly the measured key set: an upstream rename or addition that this
/// struct has not caught up with fails here, loudly, rather than reading
/// forever as "absent".
#[test]
fn the_struct_names_exactly_the_measured_keys() {
    let full: TemplateCaps =
        serde_json::from_str(MEASURED_CONFIG_A).expect("measured fixture parses");
    let value = serde_json::to_value(&full).expect("serializes");
    let mut keys: Vec<&str> = value
        .as_object()
        .expect("an object")
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, _)| k.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys, MEASURED_KEYS,
        "wire keys drifted from the measurement"
    );
}

#[test]
fn the_measured_report_parses_with_every_field_present() {
    let caps: TemplateCaps = serde_json::from_str(MEASURED_CONFIG_A).unwrap();
    assert_eq!(caps.supports_reasoning_effort, Some(true));
    assert_eq!(
        caps.supports_typed_content,
        Some(false),
        "an explicit false must survive as Some(false), not collapse to None"
    );
    let value = serde_json::to_value(&caps).unwrap();
    assert!(
        value.as_object().unwrap().values().all(|v| !v.is_null()),
        "all nine fields were on the wire, so all nine must be Some"
    );
}

/// Five caps default `true` upstream, so a key the server did not report
/// licenses no conclusion — it must parse as `None`, never as `false`.
#[test]
fn an_absent_key_stays_none_rather_than_defaulting_to_false() {
    let caps: TemplateCaps = serde_json::from_str(r#"{"supports_tools": false}"#).unwrap();
    assert_eq!(caps.supports_tools, Some(false));
    assert_eq!(caps.supports_reasoning_effort, None);
    assert_eq!(caps.supports_system_role, None);
}

/// A future build adding a tenth cap must not make the nine known ones
/// unreadable.
#[test]
fn an_unknown_key_is_ignored_rather_than_failing_the_parse() {
    let caps: TemplateCaps = serde_json::from_str(
        r#"{"supports_reasoning_effort": true, "supports_time_travel": true}"#,
    )
    .unwrap();
    assert_eq!(caps.supports_reasoning_effort, Some(true));
}

// ── The support answer ────────────────────────────────────────────────────

#[test]
fn an_observed_true_answers_yes() {
    let caps: TemplateCaps = serde_json::from_str(MEASURED_CONFIG_A).unwrap();
    assert_eq!(reasoning_effort_support(&Some(caps)), Support::Yes);
}

#[test]
fn an_observed_false_answers_no() {
    let caps = TemplateCaps {
        supports_reasoning_effort: Some(false),
        ..TemplateCaps::default()
    };
    assert_eq!(reasoning_effort_support(&Some(caps)), Support::No);
}

/// Unknown never gates, stated twice: no caps at all, and caps that did not
/// carry the field, both answer `Unknown` — never `No`.
#[test]
fn never_observed_and_field_absent_both_answer_unknown() {
    assert_eq!(reasoning_effort_support(&None), Support::Unknown);
    assert_eq!(
        reasoning_effort_support(&Some(TemplateCaps::default())),
        Support::Unknown
    );
}

// ── The tri-state ─────────────────────────────────────────────────────────

#[test]
fn the_default_state_is_not_yet_read() {
    assert_eq!(TemplateCapsState::default(), TemplateCapsState::NotYetRead);
}

#[test]
fn only_a_read_state_yields_caps() {
    let caps: TemplateCaps = serde_json::from_str(MEASURED_CONFIG_A).unwrap();
    assert!(TemplateCapsState::NotYetRead.caps().is_none());
    assert!(
        TemplateCapsState::Unreadable { reason: "x".into() }
            .caps()
            .is_none()
    );
    assert_eq!(
        TemplateCapsState::Read { caps: caps.clone() }.caps(),
        Some(&caps)
    );
}

/// The caps round-trip through the JSON the model row stores.
#[test]
fn caps_round_trip_through_serde() {
    let caps: TemplateCaps = serde_json::from_str(MEASURED_CONFIG_A).unwrap();
    let json = serde_json::to_string(&caps).unwrap();
    let back: TemplateCaps = serde_json::from_str(&json).unwrap();
    assert_eq!(caps, back);
}
