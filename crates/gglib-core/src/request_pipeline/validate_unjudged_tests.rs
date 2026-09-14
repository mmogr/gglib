//! Tests for the verdicts that forward a call unchanged: a schema the
//! validator cannot judge, a response with nothing to judge, and argument
//! shapes that are not violations. A child of `validate_tests.rs`, for its
//! fixtures.

use super::*;

// ── Unvalidatable ────────────────────────────────────────────────────────

/// A schema this validator cannot read must not be reported as conformant,
/// and must not trigger a repair either.
#[test]
fn a_schema_using_any_of_is_unvalidatable() {
    let tools = json!([{
        "type": "function",
        "function": {
            "name": "read_file",
            "parameters": {
                "type": "object",
                "properties": {"path": {"anyOf": [{"type": "string"}, {"type": "integer"}]}}
            }
        }
    }]);
    let calls = json!([{
        "type": "function",
        "function": {"name": "read_file", "arguments": r#"{"path":"a"}"#}
    }]);

    let v = validate_tool_calls(Some(&tools), Some(&calls));
    assert_eq!(v, Verdict::Unvalidatable("anyOf"));
    assert!(
        !matches!(v, Verdict::Invalid(_)),
        "must not re-roll a call it cannot judge"
    );
}

#[test]
fn a_schema_using_a_ref_is_unvalidatable() {
    let tools = json!([{
        "type": "function",
        "function": {
            "name": "read_file",
            "parameters": {"type": "object", "properties": {"p": {"$ref": "#/$defs/x"}}}
        }
    }]);
    let calls = json!([{
        "type": "function",
        "function": {"name": "read_file", "arguments": "{}"}
    }]);

    assert!(matches!(
        validate_tool_calls(Some(&tools), Some(&calls)),
        Verdict::Unvalidatable(_)
    ));
}

/// A real violation elsewhere outranks an unvalidatable schema: the repair
/// re-issues the whole turn anyway, so a known-bad call is worth acting on
/// even when a sibling cannot be judged.
#[test]
fn a_real_violation_outranks_an_unvalidatable_sibling() {
    let tools = json!([
        {"type": "function", "function": {
            "name": "weird", "parameters": {"oneOf": [{"type": "object"}]}}},
        {"type": "function", "function": {
            "name": "read_file",
            "parameters": {"type": "object", "properties": {"path": {"type": "string"}},
                           "required": ["path"]}}}
    ]);
    let calls = json!([
        {"type": "function", "function": {"name": "weird", "arguments": "{}"}},
        {"type": "function", "function": {"name": "read_file", "arguments": "{}"}}
    ]);

    let v = validate_tool_calls(Some(&tools), Some(&calls));
    assert!(matches!(v, Verdict::Invalid(_)));
    assert_eq!(kinds(&v), vec![ViolationKind::MissingRequired]);
}

// ── Not applicable ───────────────────────────────────────────────────────

#[test]
fn no_tools_is_not_applicable() {
    assert_eq!(
        validate_tool_calls(None, Some(&call("{}"))),
        Verdict::NotApplicable
    );
}

#[test]
fn no_tool_calls_is_not_applicable() {
    assert_eq!(
        validate_tool_calls(Some(&tools()), None),
        Verdict::NotApplicable
    );
}

#[test]
fn empty_arrays_are_not_applicable() {
    let empty = json!([]);
    assert_eq!(
        validate_tool_calls(Some(&empty), Some(&empty)),
        Verdict::NotApplicable
    );
}

/// Absent arguments mean an empty object, which is conformant for a tool
/// with no required properties. Treating it as malformed would repair
/// every legitimate no-argument call.
#[test]
fn absent_arguments_are_an_empty_object() {
    let tools = json!([{
        "type": "function",
        "function": {"name": "now", "parameters": {"type": "object", "properties": {}}}
    }]);
    let calls = json!([{"type": "function", "function": {"name": "now"}}]);

    assert_eq!(
        validate_tool_calls(Some(&tools), Some(&calls)),
        Verdict::Valid
    );
}

/// Some callers hand this function already-decoded arguments rather than
/// the wire's JSON string.
#[test]
fn an_object_valued_arguments_field_is_accepted() {
    let calls = json!([{
        "type": "function",
        "function": {"name": "read_file", "arguments": {"path": "a", "mode": "text"}}
    }]);

    assert_eq!(
        validate_tool_calls(Some(&tools()), Some(&calls)),
        Verdict::Valid
    );
}
