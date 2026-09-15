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

/// A tuple-shaped array is not judged. Under JSON Schema 2020-12, `items`
/// covers only the elements after the ones `prefixItems` describes, so
/// `["a", 1]` conforms to this schema; a validator that applied `items` to
/// every element would report a wrong type at `/pair/0`, the false violation
/// the module doc forbids.
#[test]
fn a_schema_using_prefix_items_is_unvalidatable() {
    let tools = json!([{
        "type": "function",
        "function": {
            "name": "t",
            "parameters": {
                "type": "object",
                "properties": {"pair": {
                    "type": "array",
                    "prefixItems": [{"type": "string"}],
                    "items": {"type": "integer"}
                }}
            }
        }
    }]);
    let calls = json!([{
        "type": "function",
        "function": {"name": "t", "arguments": r#"{"pair":["a",1]}"#}
    }]);

    assert_eq!(
        validate_tool_calls(Some(&tools), Some(&calls)),
        Verdict::Unvalidatable("prefixItems")
    );
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

// ── Where keywords are looked for ────────────────────────────────────────

/// A tool that branches can name its parameters `if`, `then` and `else`. They
/// are names under `properties`, not keywords, and the call is judged like any
/// other.
#[test]
fn a_parameter_named_like_a_keyword_is_still_validated() {
    let tools = json!([{
        "type": "function",
        "function": {
            "name": "branch",
            "parameters": {
                "type": "object",
                "properties": {
                    "if": {"type": "string"},
                    "then": {"type": "integer"},
                    "else": {"type": "integer"},
                    "not": {"type": "boolean"},
                    "definitions": {"type": "object"},
                    "$defs": {"type": "array"}
                },
                "required": ["if", "then"]
            }
        }
    }]);
    let calls = |arguments: &str| json!([{"type": "function", "function": {"name": "branch", "arguments": arguments}}]);

    assert_eq!(
        validate_tool_calls(
            Some(&tools),
            Some(&calls(r#"{"if":"x > 1","then":2,"else":3}"#))
        ),
        Verdict::Valid
    );
    let v = validate_tool_calls(Some(&tools), Some(&calls(r#"{"if":"x > 1","then":"2"}"#)));
    assert_eq!(
        kinds(&v),
        vec![ViolationKind::WrongType {
            expected: "integer".to_owned(),
            actual: "string".to_owned()
        }]
    );
}

/// A keyword inside a parameter's subschema, or under `items`, whether one
/// schema or a draft-07 tuple of them, or `additionalProperties`, is still found.
#[test]
fn a_keyword_in_a_subschema_is_still_found() {
    for parameters in [
        json!({"type": "object", "properties": {"if": {"anyOf": [{"type": "string"}]}}}),
        json!({"type": "object", "properties": {
            "paths": {"type": "array", "items": {"oneOf": [{"type": "string"}]}}
        }}),
        json!({"type": "object", "additionalProperties": {"$ref": "#/$defs/path"}}),
        json!({"type": "object", "properties": {
            "pair": {"type": "array", "items": [{"type": "string"}, {"not": {"type": "null"}}]}
        }}),
    ] {
        let tools =
            json!([{"type": "function", "function": {"name": "t", "parameters": parameters}}]);
        let calls = json!([{"type": "function", "function": {"name": "t", "arguments": "{}"}}]);
        assert!(
            matches!(
                validate_tool_calls(Some(&tools), Some(&calls)),
                Verdict::Unvalidatable(_)
            ),
            "{parameters}"
        );
    }
}

/// A value that is not a schema, such as an `enum` member or a `default`, is
/// not searched for keywords.
#[test]
fn a_value_that_is_not_a_schema_is_not_searched() {
    let tools = json!([{"type": "function", "function": {"name": "t", "parameters": {
        "type": "object",
        "properties": {
            "rule": {"type": "object", "enum": [{"if": "a"}], "default": {"$ref": "b"}}
        }
    }}}]);
    let calls = json!([{"type": "function", "function": {"name": "t", "arguments": r#"{"rule":{"if":"a"}}"#}}]);

    assert_eq!(
        validate_tool_calls(Some(&tools), Some(&calls)),
        Verdict::Valid
    );
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
