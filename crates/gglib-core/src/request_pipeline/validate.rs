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
//! `$ref`, `anyOf`/`oneOf`/`allOf`, `not`, `$defs` and `prefixItems` yield
//! [`Verdict::Unvalidatable`] and the response is forwarded untouched, and
//! `pattern` is not checked at all. Half-implementing those constructs would
//! produce false violations, and a false violation costs a wasted generation
//! and replaces a working call with a re-rolled one.
//!
//! # Recursion is not optional
//!
//! The experiment that motivated this module checked nested *presence* but not
//! nested *types*, so `options: {"follow_symlinks": "null"}` passed a
//! validator that should have rejected it and the measured conformance rate
//! came out flattering. Pinned by
//! `validate_tests::a_nested_wrong_type_is_caught` so
//! the same gap cannot reappear where it would cost a real repair.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
//! [`RuntimeCapabilities`]: crate::domain::RuntimeCapabilities

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

/// Schema keywords this validator does not implement.
///
/// Any of them in a tool's schema, or in a subschema under it, makes that call
/// unvalidatable. A parameter's name is not where a keyword goes: see
/// [`unsupported_reason`]. Listed rather than inferred so adding support for one is a
/// deliberate edit with a test, not an emergent behaviour change.
///
/// `prefixItems` is here because the array check applies `items` to every
/// element, and under a tuple schema `items` covers only the elements after
/// the prefix, so a conformant tuple would read as a violation.
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
    "prefixItems",
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
        /// The schema's declared type, or its types joined by `or` when the
        /// schema lists several.
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

/// Where a schema holds subschemas, besides the values of `properties`.
///
/// The unsupported keywords that hold subschemas are not listed: finding one
/// ends the search.
const SUBSCHEMA_KEYWORDS: &[&str] = &[
    "items",
    "additionalItems",
    "contains",
    "additionalProperties",
    "unevaluatedItems",
    "unevaluatedProperties",
];

/// The first unsupported keyword in `schema` or in a subschema under it.
///
/// A keyword is looked for only where a schema puts keywords. The keys of
/// `properties` are a tool's parameter names, so a parameter called `if` or
/// `definitions` is a name, and only the subschema under it is searched. A
/// value that is not a subschema, such as an `enum` member or a `default`, is
/// not searched at all.
fn unsupported_reason(schema: &Value) -> Option<&'static str> {
    let map = schema.as_object()?;
    if let Some(keyword) = UNSUPPORTED_KEYWORDS
        .iter()
        .copied()
        .find(|keyword| map.contains_key(*keyword))
    {
        return Some(keyword);
    }
    let named = map
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(serde_json::Map::values);
    let positional = SUBSCHEMA_KEYWORDS
        .iter()
        .filter_map(|keyword| map.get(*keyword));
    named
        .chain(positional)
        .find_map(|subschema| match subschema {
            Value::Array(tuple) => tuple.iter().find_map(unsupported_reason),
            one => unsupported_reason(one),
        })
}

/// Check `value` against `schema`, appending `(pointer, kind)` for each
/// violation found at or below this point.
fn check_value(
    value: &Value,
    schema: &Value,
    pointer: &str,
    out: &mut Vec<(String, ViolationKind)>,
) {
    if let Some(expected) = schema.get("type")
        && !type_satisfied(value, expected)
    {
        out.push((
            pointer.to_owned(),
            ViolationKind::WrongType {
                expected: type_label(expected),
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

/// Whether `value` satisfies a `type` keyword, written as one type or a list.
///
/// A list accepts a value of any type in it, as `["string", "null"]` does for
/// an optional field. A list that names no type, or a `type` that is neither a
/// name nor a list, is not a constraint this validator reads, and passes.
fn type_satisfied(value: &Value, expected: &Value) -> bool {
    match expected {
        Value::String(name) => type_matches(value, name),
        Value::Array(listed) => {
            let mut types = listed.iter().filter_map(Value::as_str).peekable();
            types.peek().is_none() || types.any(|name| type_matches(value, name))
        }
        _ => true,
    }
}

/// The `type` keyword as a violation names it: `integer`, or `string or null`.
fn type_label(expected: &Value) -> String {
    match expected {
        Value::Array(names) => names
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" or "),
        other => other.as_str().unwrap_or_default().to_owned(),
    }
}

/// Whether `value` satisfies one JSON Schema type name.
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
        // satisfied rather than fail a call over a keyword we do not model,
        // and say so, so a schema that turns validation off for a field can be
        // found.
        unknown => {
            debug!(
                r#type = unknown,
                "tool schema names a type this validator does not know; not checked"
            );
            true
        }
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
#[path = "validate_tests.rs"]
mod validate_tests;
