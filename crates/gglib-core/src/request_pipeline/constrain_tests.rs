//! Tests for the grammar origination in `constrain.rs`.
//!
//! Split from `constrain.rs` so the module stays inside the complexity
//! ratchet's budget — the repo's `*_tests.rs` sibling pattern.

use super::*;
use crate::normalize::tags::FORMAT_QWEN_XML;
use serde_json::json;

fn qwen_ctx() -> ModelContext {
    ModelContext {
        tags: vec![FORMAT_QWEN_XML.to_owned()],
        dialect: Some(DialectSpec::qwen_xml()),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    }
}

fn body(tool_choice: &Value) -> Value {
    json!({
        "model": "m",
        "messages": [{"role": "user", "content": "hi"}],
        "tools": [
            {"type": "function", "function": {"name": "read_file", "parameters": {}}},
            {"type": "function", "function": {"name": "ls", "parameters": {}}},
        ],
        "tool_choice": tool_choice,
    })
}

/// The case the stage exists for: a demanded call on a dialect model
/// gets a grammar, and `tool_choice` is rewritten to the one value
/// llama-server accepts alongside it.
#[test]
fn required_installs_a_grammar_and_rewrites_tool_choice() {
    let mut b = body(&json!("required"));
    assert!(constrain_tool_calls_inner(&mut b, &qwen_ctx()));

    let grammar = b["grammar"].as_str().unwrap();
    assert!(grammar.contains(r#""\"read_file\"" | "\"ls\"""#));
    assert!(grammar.contains(r#""<tool_call>""#));
    assert_eq!(b["tool_choice"], "none");
    assert!(
        b.get("tools").is_some(),
        "tools stay for template rendering"
    );
}

/// A named function narrows the grammar to that one tool.
#[test]
fn a_named_function_constrains_to_that_name() {
    let mut b = body(&json!({"type": "function", "function": {"name": "ls"}}));
    assert!(constrain_tool_calls_inner(&mut b, &qwen_ctx()));

    let grammar = b["grammar"].as_str().unwrap();
    assert!(grammar.contains(r#"name ::= "\"ls\"""#));
    assert!(!grammar.contains("read_file"));
}

/// `auto` (and absent) must stay unconstrained — a start-anchored
/// grammar would forbid the plain-text answers `auto` permits.
#[test]
fn auto_and_absent_are_left_alone() {
    for tc in [json!("auto"), Value::Null] {
        let mut b = body(&tc);
        if b["tool_choice"].is_null() {
            b.as_object_mut().unwrap().remove("tool_choice");
        }
        assert!(!constrain_tool_calls_inner(&mut b, &qwen_ctx()));
        assert!(b.get("grammar").is_none());
    }
}

/// A client that constrains its own decode is always respected.
#[test]
fn client_constraints_are_never_overwritten() {
    for (key, value) in [
        ("grammar", json!("root ::= \"x\"")),
        ("json_schema", json!({"type": "object"})),
        ("response_format", json!({"type": "json_object"})),
    ] {
        let mut b = body(&json!("required"));
        b[key] = value.clone();
        assert!(!constrain_tool_calls_inner(&mut b, &qwen_ctx()));
        assert_eq!(b[key], value, "{key} must be untouched");
        assert_eq!(b["tool_choice"], "required");
    }
}

/// Only the dialect the proxy parses is constrained; native-path models
/// belong to llama-server's own tool machinery.
#[test]
fn non_dialect_and_unresolved_models_are_skipped() {
    let untagged = ModelContext {
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let unresolved = ModelContext {
        tags: vec![FORMAT_QWEN_XML.to_owned()],
        dialect: Some(DialectSpec::qwen_xml()),
        ..ModelContext::passthrough()
    };
    for ctx in [untagged, unresolved] {
        let mut b = body(&json!("required"));
        assert!(!constrain_tool_calls_inner(&mut b, &ctx));
        assert!(b.get("grammar").is_none());
    }
}

/// A demand for a tool that was never advertised is forwarded as-is for
/// llama-server (or the model) to answer, not constrained to a name the
/// prompt does not contain.
#[test]
fn a_named_function_not_in_tools_is_not_constrained() {
    let mut b = body(&json!({"type": "function", "function": {"name": "ghost"}}));
    assert!(!constrain_tool_calls_inner(&mut b, &qwen_ctx()));
}

/// Names that would need escaping inside a GBNF literal abort the whole
/// constraint rather than risk an uncompilable grammar.
#[test]
fn unsafe_tool_names_abort_constraint() {
    let mut b = body(&json!("required"));
    b["tools"][0]["function"]["name"] = json!("evil\"name");
    assert!(!constrain_tool_calls_inner(&mut b, &qwen_ctx()));
    assert!(b.get("grammar").is_none());
}

/// `tool_choice: "none"` is an explicit opt-out, not a demand.
#[test]
fn tool_choice_none_is_untouched() {
    let mut b = body(&json!("none"));
    assert!(!constrain_tool_calls_inner(&mut b, &qwen_ctx()));
}

/// The generated grammar parses back through the `DelimitedToolCallParser`:
/// what the grammar admits, the proxy's own parser must extract. The
/// emission comes from the *spec itself* (`render_call`), so grammar,
/// parser, and test all read one source — no hand-maintained sample to
/// silently drift.
/// The regression guard for the 2026-08-29 runaway. `(sp call)*` let a
/// 4B model emit 606 calls for a one-call task, stopping only when it
/// exhausted a 32,768 context — 853s against 6s for the same model
/// decoding unconstrained, and scored 1.0 both ways.
#[test]
fn the_root_rule_bounds_how_many_calls_it_can_express() {
    let spec = DialectSpec::qwen_xml();
    let g = tool_call_grammar(&spec, &["f".to_owned()], 3).unwrap();
    let root = g.lines().next().expect("a root rule");
    assert!(
        !root.contains("(sp call)*"),
        "unbounded repetition is the defect itself: {root}"
    );
    assert_eq!(
        root.matches("(sp call)?").count(),
        2,
        "a limit of 3 is one required call plus two optional ones: {root}"
    );
}

/// A limit of one must not emit a stray `(sp call)?` — the off-by-one that
/// would quietly permit two calls where the caller asked for one.
#[test]
fn a_limit_of_one_expresses_exactly_one_call() {
    let g = tool_call_grammar(&DialectSpec::qwen_xml(), &["f".to_owned()], 1).unwrap();
    let root = g.lines().next().expect("a root rule");
    assert!(!root.contains("(sp call)?"), "no optional repeat: {root}");
    assert!(root.starts_with("root ::= sp call sp"), "got: {root}");
}

/// Zero is not a grammar that can express a demanded call, so it is
/// clamped rather than honoured — `saturating_sub` must not underflow
/// into a repeat count of `usize::MAX`.
#[test]
fn a_limit_of_zero_still_expresses_one_call() {
    let g = tool_call_grammar(&DialectSpec::qwen_xml(), &["f".to_owned()], 0).unwrap();
    let root = g.lines().next().expect("a root rule");
    assert!(root.starts_with("root ::= sp call sp"), "got: {root}");
}

/// The default is the ceiling, because no configured `max_parallel_tools`
/// may exceed it — so anything past it is generation the loop is certain
/// to discard.
#[test]
fn the_default_bound_is_the_parallel_tools_ceiling() {
    assert_eq!(grammar_call_limit(), MAX_PARALLEL_TOOLS_CEILING);
}

#[test]
fn grammar_shape_round_trips_through_the_parser() {
    use crate::normalize::get_parser;

    for spec in [
        DialectSpec::qwen_xml(),
        DialectSpec {
            tool_open: "«TC»".to_owned(),
            tool_close: "«/TC»".to_owned(),
            ..DialectSpec::qwen_xml()
        },
    ] {
        // The canonical string the grammar's `call` rule admits.
        let emission = spec.render_call("read_file", &json!({"path": "a.rs"}));

        let grammar =
            tool_call_grammar(&spec, &["read_file".to_owned()], MAX_PARALLEL_TOOLS_CEILING)
                .unwrap();
        assert!(grammar.contains(&format!("\"{}\"", spec.tool_open)));

        let mut parser = get_parser(Some(&spec));
        let mut out = parser.push_text(&emission);
        let fin = parser.finish();
        out.tool_calls.extend(fin.tool_calls);
        out.errors.extend(fin.errors);

        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "read_file");
        assert_eq!(out.tool_calls[0].arguments["path"], "a.rs");
    }
}

/// A spec without the JSON codec cannot be grammar-enforced — the
/// grammar can only originate JSON-shaped bodies.
#[test]
fn a_function_xml_only_spec_never_installs_a_grammar() {
    use crate::domain::dialect::BodyCodec;
    let ctx = ModelContext {
        dialect: Some(DialectSpec {
            body_codecs: vec![BodyCodec::FunctionXml],
            ..DialectSpec::qwen_xml()
        }),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let mut b = body(&json!("required"));
    assert!(!constrain_tool_calls_inner(&mut b, &ctx));
    assert!(b.get("grammar").is_none());
}

/// Markers with quotes or backslashes are escaped into the literal;
/// control bytes abort the constraint entirely.
#[test]
fn marker_escaping_and_control_byte_bailout() {
    let quoted = DialectSpec {
        tool_open: "<\"q\">".to_owned(),
        ..DialectSpec::qwen_xml()
    };
    let grammar =
        tool_call_grammar(&quoted, &["f".to_owned()], MAX_PARALLEL_TOOLS_CEILING).unwrap();
    assert!(grammar.contains(r#""<\"q\">""#));

    let control = DialectSpec {
        tool_open: "<a\u{1}b>".to_owned(),
        ..DialectSpec::qwen_xml()
    };
    assert_eq!(
        tool_call_grammar(&control, &["f".to_owned()], MAX_PARALLEL_TOOLS_CEILING),
        None
    );

    let ctx = ModelContext {
        dialect: Some(control),
        catalog_resolved: true,
        ..ModelContext::passthrough()
    };
    let mut b = body(&json!("required"));
    assert!(!constrain_tool_calls_inner(&mut b, &ctx));
    assert!(b.get("grammar").is_none());
}

/// The emission profile drives the grammar's newline tokens.
#[test]
fn emission_profile_controls_grammar_newlines() {
    use crate::domain::dialect::EmissionProfile;
    let no_newlines = DialectSpec {
        emission: EmissionProfile {
            newline_after_open: false,
            newline_before_close: false,
        },
        ..DialectSpec::qwen_xml()
    };
    let grammar =
        tool_call_grammar(&no_newlines, &["f".to_owned()], MAX_PARALLEL_TOOLS_CEILING).unwrap();
    assert!(grammar.contains(r#"call ::= "<tool_call>" "{""#));

    let with_newlines = tool_call_grammar(
        &DialectSpec::qwen_xml(),
        &["f".to_owned()],
        MAX_PARALLEL_TOOLS_CEILING,
    )
    .unwrap();
    assert!(with_newlines.contains(r#"call ::= "<tool_call>" nl "{""#));
}
