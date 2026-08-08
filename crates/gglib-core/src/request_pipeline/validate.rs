//! Do the emitted tool calls actually match the schemas the client advertised?
//!
//! **Tier B — Policy** ([ADR 0001]). llama-server has no view of what a client
//! does with a malformed call, so deciding whether to forward one is gglib's
//! call regardless of how good upstream's grammar becomes. Nothing here is
//! gated on [`RuntimeCapabilities`].
//!
//! # Why this exists
//!
//! `tool_choice: "auto"` is the path every agentic client uses, and on some
//! model/build pairs llama.cpp installs no grammar for it. Measured on
//! `b10327` ([ADR 0002], findings 4-5):
//!
//! | model | `auto` conformance | `required` conformance |
//! |---|---|---|
//! | Qwen3.5-4B | 30/30 | 30/30 |
//! | Llama 3.2 3B | **≤ 4/30** | 30/30 |
//!
//! On Llama 3.2, 26 of 30 calls put `max_lines` as the string `"42"` where the
//! schema declares an integer. The client's executor then fails, reports the
//! error back to the model, and the model tries again — one of the ways a
//! local agentic session dies, and nothing in gglib noticed it happening.
//!
//! This module is the detection half. The repair half — re-issuing with
//! `tool_choice: "required"`, which is where upstream *does* install a
//! grammar — lives in the proxy, because only it can make a second request.
//! See [Tool-call repair](https://github.com/mmogr/gglib/blob/main/docs/tool-call-repair.md).
//!
//! # Deliberately not a JSON Schema engine
//!
//! Only the constraint kinds small models demonstrably get wrong are checked:
//! types, `required`, `enum`, `additionalProperties: false`, and the same
//! checks recursively through nested objects and array items.
//!
//! Everything else — `$ref`, `anyOf`/`oneOf`/`allOf`, `not`, `pattern`,
//! `$defs` — yields [`Verdict::Unvalidatable`] and the response is forwarded
//! untouched. Half-implementing those constructs would produce false
//! violations, and a false violation costs a wasted generation and replaces a
//! working call with a re-rolled one.
//!
//! # Recursion is not optional
//!
//! The experiment that motivated this module checked nested *presence* but not
//! nested *types*, so `options: {"follow_symlinks": "null"}` passed a
//! validator that should have rejected it and the measured conformance rate
//! came out flattering. Pinned by
//! [`a_nested_wrong_type_is_caught`](tests::a_nested_wrong_type_is_caught) so
//! the same gap cannot reappear where it would cost a real repair.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
//! [`RuntimeCapabilities`]: crate::domain::RuntimeCapabilities

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Schema keywords this validator does not implement.
///
/// Presence of any of them anywhere in a tool's schema makes that call
/// unvalidatable. Listed rather than inferred so adding support for one is a
/// deliberate edit with a test, not an emergent behaviour change.
const UNSUPPORTED_KEYWORDS: &[&str] = &[
    "$ref",
    "$defs",
    "definitions",
    "anyOf",
    "oneOf",
    "allOf",
    "not",
    "if",
    "then",
    "else",
    "patternProperties",
    "dependentSchemas",
    "propertyNames",
];

/// What a single tool call got wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// Index into the response's `tool_calls` array.
    pub call_index: usize,
    /// Function name as the call reported it.
    pub function: String,
    /// JSON-pointer-ish path to the offending value within `arguments`.
    ///
    /// Empty string for a violation about the arguments object as a whole.
    /// `/options/follow_symlinks` for a nested one — the path is what makes a
    /// recorded violation actionable rather than merely a count.
    pub pointer: String,
    /// What kind of constraint was broken.
    pub kind: ViolationKind,
}

/// The constraint a [`Violation`] broke.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ViolationKind {
    /// `arguments` was not parseable JSON.
    MalformedArguments,
    /// `arguments` parsed but was not a JSON object.
    ArgumentsNotObject,
    /// The call named a function absent from the advertised `tools`.
    UnknownFunction,
    /// A `required` property was absent.
    MissingRequired,
    /// A value's JSON type did not match the schema's `type`.
    WrongType {
        /// The schema's declared type.
        expected: String,
        /// The type actually observed.
        actual: String,
    },
    /// A value was not a member of the schema's `enum`.
    NotInEnum,
    /// A property was present that the schema does not declare, under
    /// `additionalProperties: false`.
    UnexpectedProperty,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let where_ = if self.pointer.is_empty() {
            "arguments".to_owned()
        } else {
            self.pointer.clone()
        };
        match &self.kind {
            ViolationKind::MalformedArguments => {
                write!(f, "{}: arguments are not valid JSON", self.function)
            }
            ViolationKind::ArgumentsNotObject => {
                write!(f, "{}: arguments are not an object", self.function)
            }
            ViolationKind::UnknownFunction => {
                write!(f, "{}: not an advertised tool", self.function)
            }
            ViolationKind::MissingRequired => {
                write!(f, "{}: {where_} is required but absent", self.function)
            }
            ViolationKind::WrongType { expected, actual } => write!(
                f,
                "{}: {where_} is {actual}, schema says {expected}",
                self.function
            ),
            ViolationKind::NotInEnum => {
                write!(
                    f,
                    "{}: {where_} is not one of the allowed values",
                    self.function
                )
            }
            ViolationKind::UnexpectedProperty => {
                write!(f, "{}: {where_} is not a declared property", self.function)
            }
        }
    }
}

/// The outcome of validating one response's tool calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Every call conforms to its tool's schema.
    Valid,
    /// At least one call violates its schema.
    Invalid(Vec<Violation>),
    /// No violations found, but at least one call's schema uses a construct
    /// this validator does not implement, so "no violations" is not a claim
    /// worth acting on.
    Unvalidatable(&'static str),
    /// The request advertised no tools, or the response contained no calls.
    NotApplicable,
}

impl Verdict {
    /// Whether this verdict should trigger a repair attempt.
    ///
    /// Only [`Verdict::Invalid`]. [`Verdict::Unvalidatable`] deliberately does
    /// not: re-rolling a call that may well be correct, because gglib cannot
    /// read its schema, spends a generation to trade a working call for a
    /// different one.
    #[must_use]
    pub const fn warrants_repair(&self) -> bool {
        matches!(self, Self::Invalid(_))
    }

    /// The violations, empty for every other verdict.
    #[must_use]
    pub fn violations(&self) -> &[Violation] {
        match self {
            Self::Invalid(v) => v,
            _ => &[],
        }
    }
}

/// Validate a response's `tool_calls` against the request's `tools`.
///
/// Both arguments are the raw arrays in `OpenAI` shape: `tools` as the client
/// sent it, `tool_calls` as the response carried it (with `arguments` still a
/// JSON-encoded string, which this function parses).
///
/// Never panics and never errors — an input it cannot make sense of yields
/// [`Verdict::NotApplicable`] or [`Verdict::Unvalidatable`], both of which mean
/// *forward unchanged*.
#[must_use]
pub fn validate_tool_calls(tools: Option<&Value>, tool_calls: Option<&Value>) -> Verdict {
    let (Some(tools), Some(calls)) = (
        tools.and_then(Value::as_array),
        tool_calls.and_then(Value::as_array),
    ) else {
        return Verdict::NotApplicable;
    };

    if tools.is_empty() || calls.is_empty() {
        return Verdict::NotApplicable;
    }

    let mut violations = Vec::new();
    let mut unvalidatable: Option<&'static str> = None;

    for (index, call) in calls.iter().enumerate() {
        let function = call
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();

        let push = |violations: &mut Vec<Violation>, pointer: &str, kind: ViolationKind| {
            violations.push(Violation {
                call_index: index,
                function: function.clone(),
                pointer: pointer.to_owned(),
                kind,
            });
        };

        let Some(schema) = schema_for(tools, &function) else {
            push(&mut violations, "", ViolationKind::UnknownFunction);
            continue;
        };

        if let Some(reason) = unsupported_reason(schema) {
            unvalidatable = unvalidatable.or(Some(reason));
            continue;
        }

        let raw = call.get("function").and_then(|f| f.get("arguments"));
        let parsed = match raw {
            // Already-decoded arguments: some paths hand this function an
            // object rather than the wire's JSON string.
            Some(Value::Object(_)) => raw.cloned(),
            Some(Value::String(s)) => serde_json::from_str::<Value>(s).ok(),
            // Absent arguments are an empty object, not a violation: a tool
            // with no required properties is legitimately called with none.
            None | Some(Value::Null) => Some(Value::Object(serde_json::Map::new())),
            _ => None,
        };

        let Some(parsed) = parsed else {
            push(&mut violations, "", ViolationKind::MalformedArguments);
            continue;
        };

        if !parsed.is_object() {
            push(&mut violations, "", ViolationKind::ArgumentsNotObject);
            continue;
        }

        let mut found = Vec::new();
        check_value(&parsed, schema, "", &mut found);
        for (pointer, kind) in found {
            violations.push(Violation {
                call_index: index,
                function: function.clone(),
                pointer,
                kind,
            });
        }
    }

    if !violations.is_empty() {
        return Verdict::Invalid(violations);
    }
    unvalidatable.map_or(Verdict::Valid, Verdict::Unvalidatable)
}

/// The `parameters` schema for `name`, from the advertised tools.
fn schema_for<'a>(tools: &'a [Value], name: &str) -> Option<&'a Value> {
    tools
        .iter()
        .filter_map(|t| t.get("function"))
        .find(|f| f.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|f| f.get("parameters"))
}

/// The first unsupported keyword anywhere in `schema`, if any.
fn unsupported_reason(schema: &Value) -> Option<&'static str> {
    match schema {
        Value::Object(map) => {
            for key in UNSUPPORTED_KEYWORDS {
                if map.contains_key(*key) {
                    return Some(key);
                }
            }
            map.values().find_map(unsupported_reason)
        }
        Value::Array(items) => items.iter().find_map(unsupported_reason),
        _ => None,
    }
}

/// Check `value` against `schema`, appending `(pointer, kind)` for each
/// violation found at or below this point.
fn check_value(
    value: &Value,
    schema: &Value,
    pointer: &str,
    out: &mut Vec<(String, ViolationKind)>,
) {
    if let Some(expected) = schema.get("type").and_then(Value::as_str)
        && !type_matches(value, expected)
    {
        out.push((
            pointer.to_owned(),
            ViolationKind::WrongType {
                expected: expected.to_owned(),
                actual: type_name(value).to_owned(),
            },
        ));
        // A value of the wrong type cannot meaningfully be checked against the
        // schema's other constraints — reporting "not in enum" about a string
        // that should have been an object is noise, not a second finding.
        return;
    }

    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        out.push((pointer.to_owned(), ViolationKind::NotInEnum));
    }

    match value {
        Value::Object(map) => check_object(map, schema, pointer, out),
        Value::Array(items) => {
            if let Some(item_schema) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    check_value(item, item_schema, &format!("{pointer}/{i}"), out);
                }
            }
        }
        _ => {}
    }
}

/// The object-shaped checks: `required`, declared properties, and
/// `additionalProperties: false`.
fn check_object(
    map: &serde_json::Map<String, Value>,
    schema: &Value,
    pointer: &str,
    out: &mut Vec<(String, ViolationKind)>,
) {
    let props = schema.get("properties").and_then(Value::as_object);

    for key in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !map.contains_key(key) {
            out.push((format!("{pointer}/{key}"), ViolationKind::MissingRequired));
        }
    }

    // Only an explicit `false` forbids extras; an absent `additionalProperties`
    // permits them, per JSON Schema.
    let extras_forbidden = schema.get("additionalProperties") == Some(&Value::Bool(false));

    for (key, child) in map {
        let child_pointer = format!("{pointer}/{key}");
        match props.and_then(|p| p.get(key)) {
            Some(child_schema) => check_value(child, child_schema, &child_pointer, out),
            None if extras_forbidden => {
                out.push((child_pointer, ViolationKind::UnexpectedProperty));
            }
            None => {}
        }
    }
}

/// Whether `value` satisfies a JSON Schema `type` keyword.
///
/// `integer` accepts a float whose fractional part is zero, which JSON Schema
/// requires and which matters because a model emitting `3.0` for a count is
/// producing a valid integer, not a violation.
fn type_matches(value: &Value, expected: &str) -> bool {
    match expected {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "integer" => {
            value.as_i64().is_some()
                || value.as_u64().is_some()
                || value.as_f64().is_some_and(|f| f.fract() == 0.0)
        }
        // An unrecognised `type` is not a violation to invent — treat it as
        // satisfied rather than fail a call over a keyword we do not model.
        _ => true,
    }
}

/// The JSON type name of `value`, for violation reporting.
const fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
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

        assert!(v.warrants_repair());
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
        let v =
            verdict(r#"{"path":"/etc/hosts","mode":"text","options":{"follow_symlinks":"null"}}"#);

        assert!(v.warrants_repair());
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
        assert!(v.warrants_repair());
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
            !v.warrants_repair(),
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
        assert!(v.warrants_repair());
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

    #[test]
    fn a_violation_renders_a_useful_message() {
        let v = verdict(r#"{"path":"a","mode":"text","max_lines":"42"}"#);
        let rendered = v.violations()[0].to_string();

        assert!(rendered.contains("read_file"), "{rendered}");
        assert!(rendered.contains("/max_lines"), "{rendered}");
        assert!(rendered.contains("integer"), "{rendered}");
    }
}
