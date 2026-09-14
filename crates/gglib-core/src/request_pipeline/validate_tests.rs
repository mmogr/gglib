//! Tests for [`super`]: the violations the validator reports.
//!
//! Moved out of `validate.rs` as they were, to keep that file within the Rust
//! size ratchet. The verdicts that forward a call unchanged are in
//! `validate_unjudged_tests.rs`, a child of this module that shares its
//! fixtures.

use super::*;
use serde_json::json;

/// The schema from the conformance experiment, so the tests exercise the
/// exact shape the measurements were taken against.
fn tools() -> Value {
    json!([{
        "type": "function",
        "function": {
            "name": "read_file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "max_lines": {"type": "integer"},
                    "mode": {"type": "string", "enum": ["text", "binary"]},
                    "options": {
                        "type": "object",
                        "properties": {"follow_symlinks": {"type": "boolean"}},
                        "required": ["follow_symlinks"]
                    }
                },
                "required": ["path", "mode"],
                "additionalProperties": false
            }
        }
    }])
}

fn call(args: &str) -> Value {
    json!([{
        "type": "function",
        "function": {"name": "read_file", "arguments": args}
    }])
}

fn verdict(args: &str) -> Verdict {
    validate_tool_calls(Some(&tools()), Some(&call(args)))
}

fn kinds(v: &Verdict) -> Vec<ViolationKind> {
    v.violations().iter().map(|x| x.kind.clone()).collect()
}

#[test]
fn a_conformant_call_is_valid() {
    assert_eq!(
        verdict(r#"{"path":"/etc/hosts","mode":"text"}"#),
        Verdict::Valid
    );
}

/// The exact violation measured on Llama 3.2: an integer field carrying a
/// string. 26 of 30 calls looked like this.
#[test]
fn the_llama_32_failure_is_caught() {
    let v = verdict(r#"{"path":"42","mode":"text","max_lines":"42"}"#);

    assert!(matches!(v, Verdict::Invalid(_)));
    assert_eq!(
        kinds(&v),
        vec![ViolationKind::WrongType {
            expected: "integer".to_owned(),
            actual: "string".to_owned()
        }]
    );
    assert_eq!(v.violations()[0].pointer, "/max_lines");
}

/// The gap the experiment harness had: nested presence was checked, nested
/// types were not, so this passed and the measured rate came out
/// flattering. See the module docs.
#[test]
fn a_nested_wrong_type_is_caught() {
    let v = verdict(r#"{"path":"/etc/hosts","mode":"text","options":{"follow_symlinks":"null"}}"#);

    assert!(matches!(v, Verdict::Invalid(_)));
    assert_eq!(v.violations()[0].pointer, "/options/follow_symlinks");
    assert_eq!(
        kinds(&v),
        vec![ViolationKind::WrongType {
            expected: "boolean".to_owned(),
            actual: "string".to_owned()
        }]
    );
}

#[test]
fn a_missing_required_property_is_caught() {
    let v = verdict(r#"{"path":"/etc/hosts"}"#);
    assert_eq!(kinds(&v), vec![ViolationKind::MissingRequired]);
    assert_eq!(v.violations()[0].pointer, "/mode");
}

#[test]
fn a_missing_nested_required_property_is_caught() {
    let v = verdict(r#"{"path":"/etc/hosts","mode":"text","options":{}}"#);
    assert_eq!(kinds(&v), vec![ViolationKind::MissingRequired]);
    assert_eq!(v.violations()[0].pointer, "/options/follow_symlinks");
}

#[test]
fn a_value_outside_its_enum_is_caught() {
    let v = verdict(r#"{"path":"/etc/hosts","mode":"fast"}"#);
    assert_eq!(kinds(&v), vec![ViolationKind::NotInEnum]);
}

#[test]
fn an_undeclared_property_is_caught_under_additional_properties_false() {
    let v = verdict(r#"{"path":"/etc/hosts","mode":"text","recursive":true}"#);
    assert_eq!(kinds(&v), vec![ViolationKind::UnexpectedProperty]);
    assert_eq!(v.violations()[0].pointer, "/recursive");
}

/// Absent `additionalProperties` permits extras, per JSON Schema. Flagging
/// them would repair calls that are correct.
#[test]
fn an_undeclared_property_is_allowed_when_additional_properties_is_absent() {
    let tools = json!([{
        "type": "function",
        "function": {
            "name": "read_file",
            "parameters": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"]
            }
        }
    }]);
    let calls = json!([{
        "type": "function",
        "function": {"name": "read_file", "arguments": r#"{"path":"a","extra":1}"#}
    }]);

    assert_eq!(
        validate_tool_calls(Some(&tools), Some(&calls)),
        Verdict::Valid
    );
}

#[test]
fn malformed_arguments_json_is_a_violation() {
    let v = verdict(r#"{"path":"/etc/hosts","mode":"te"#);
    assert_eq!(kinds(&v), vec![ViolationKind::MalformedArguments]);
}

#[test]
fn arguments_that_are_not_an_object_are_a_violation() {
    let v = verdict(r#"["/etc/hosts"]"#);
    assert_eq!(kinds(&v), vec![ViolationKind::ArgumentsNotObject]);
}

#[test]
fn a_call_to_an_unadvertised_tool_is_a_violation() {
    let calls = json!([{
        "type": "function",
        "function": {"name": "delete_everything", "arguments": "{}"}
    }]);
    let v = validate_tool_calls(Some(&tools()), Some(&calls));
    assert_eq!(kinds(&v), vec![ViolationKind::UnknownFunction]);
}

/// A bool must not satisfy `integer`. Trivial in Rust, load-bearing in the
/// Python harness where `bool` subclasses `int` — pinned so a future port
/// of this logic cannot reintroduce it.
#[test]
fn a_boolean_does_not_satisfy_integer() {
    let v = verdict(r#"{"path":"a","mode":"text","max_lines":true}"#);
    assert_eq!(
        kinds(&v),
        vec![ViolationKind::WrongType {
            expected: "integer".to_owned(),
            actual: "boolean".to_owned()
        }]
    );
}

/// JSON Schema counts a zero-fraction float as an integer.
#[test]
fn a_whole_float_satisfies_integer() {
    assert_eq!(
        verdict(r#"{"path":"a","mode":"text","max_lines":3.0}"#),
        Verdict::Valid
    );
}

#[test]
fn a_fractional_float_does_not_satisfy_integer() {
    let v = verdict(r#"{"path":"a","mode":"text","max_lines":3.5}"#);
    assert!(matches!(v, Verdict::Invalid(_)));
}

/// One wrong-typed value yields one finding, not a cascade of unrelated
/// ones about constraints that cannot apply to it.
#[test]
fn a_wrong_type_suppresses_downstream_checks_on_the_same_value() {
    let v = verdict(r#"{"path":"a","mode":42}"#);
    assert_eq!(
        kinds(&v),
        vec![ViolationKind::WrongType {
            expected: "string".to_owned(),
            actual: "number".to_owned()
        }],
        "should not also report NotInEnum for a value that is not a string"
    );
}

#[test]
fn several_violations_across_one_call_are_all_reported() {
    let v = verdict(r#"{"mode":"fast","recursive":true}"#);
    assert_eq!(v.violations().len(), 3, "missing path, bad enum, extra key");
}

#[test]
fn violations_carry_the_index_of_the_call_that_produced_them() {
    let calls = json!([
        {"type": "function", "function": {"name": "read_file", "arguments": r#"{"path":"a","mode":"text"}"#}},
        {"type": "function", "function": {"name": "read_file", "arguments": r#"{"path":"b"}"#}}
    ]);
    let v = validate_tool_calls(Some(&tools()), Some(&calls));

    assert_eq!(v.violations().len(), 1);
    assert_eq!(v.violations()[0].call_index, 1);
}

#[test]
fn a_violation_renders_a_useful_message() {
    let v = verdict(r#"{"path":"a","mode":"text","max_lines":"42"}"#);
    let rendered = v.violations()[0].to_string();

    assert!(rendered.contains("read_file"), "{rendered}");
    assert!(rendered.contains("/max_lines"), "{rendered}");
    assert!(rendered.contains("integer"), "{rendered}");
}

#[path = "validate_unjudged_tests.rs"]
mod validate_unjudged_tests;
