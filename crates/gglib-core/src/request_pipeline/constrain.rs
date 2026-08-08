//! Stage 6: decode-time enforcement of dialect tool calls.
//!
//! **Tier A — Compensation** ([ADR 0001]). This stage originates a grammar
//! because llama.cpp builds none for dialect models. It is the clearest
//! deletion candidate in the tree, because upstream has the machinery
//! already: `json_schema_to_grammar` converts tool schemas to GBNF, and
//! lazily-triggered grammars exist for the `auto` case gglib cannot cover.
//!
//! *Deletion criterion:* llama.cpp constrains dialect tool calls under both
//! `tool_choice: "required"` and `"auto"`, with arguments conforming to the
//! tool's own JSON Schema rather than merely being well-formed JSON. Note
//! that this stage's grammar is *weaker* than that today — it constrains the
//! envelope, the function name, and JSON well-formedness, but admits
//! `{"path": 42}` against a schema demanding a string. So the criterion is
//! not "upstream matches this stage" but "upstream exceeds it", and meeting
//! it deletes this stage and obviates the schema-constraint work it would
//! otherwise need.
//!
//! Measured by `scripts/experiments/lazy_grammar_conformance.py`, whose
//! result is recorded as its own ADR rather than assumed from
//! [`RuntimeFlags::PEG_NATIVE_TOOL_CALLS`].
//!
//! **Criterion met, stage retained** ([ADR 0002]). On `b10327` against
//! Qwen3.5-4B, upstream held 60/60 across `auto` and `required` under prompts
//! written to break types, enums, required fields and `additionalProperties`.
//! It exceeds this stage on the measured path, and the schema-constraint work
//! this stage would otherwise have needed is dropped rather than deferred.
//!
//! The stage stays anyway, and the reason is the scope of the evidence: one
//! model, one build, one schema. Deleting a stage that also serves dialects
//! nobody has measured would trade a known cost for an unmeasured risk — the
//! same asymmetry [`RuntimeCapabilities::unknown`] encodes. What remains
//! before removal is a second dialect family measured to the same standard.
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [ADR 0002]: https://github.com/mmogr/gglib/blob/main/docs/adr/0002-defer-tool-call-constraint-to-llama-cpp.md
//! [`RuntimeFlags::PEG_NATIVE_TOOL_CALLS`]: crate::domain::RuntimeFlags::PEG_NATIVE_TOOL_CALLS
//! [`RuntimeCapabilities::unknown`]: crate::domain::RuntimeCapabilities::unknown
//!
//! For models with a resolved [`DialectSpec`], tool calls are free text —
//! the model *chooses* to emit `OPEN{json}CLOSE` markup and the proxy
//! parses it after the fact. Post-hoc parsing can rescue a well-formed call,
//! but it cannot stop a small model from producing a malformed one. This
//! stage can: when the client *demands* a tool call (`tool_choice:
//! "required"` or a named function), it originates a GBNF `grammar` that
//! llama-server enforces at decode time, making an invalid envelope,
//! invalid JSON, or an invented tool name unrepresentable. The grammar is
//! generated from the same spec the parser reads, so enforcement and
//! parsing cannot drift.
//!
//! # Why only dialect models
//!
//! Models whose chat template does native tool handling are already
//! constrained: llama.cpp builds its own grammar from the template (eager
//! under `required`, lazily-triggered under `auto`) — and its `OpenAI`
//! endpoint *rejects* a request that combines a custom `grammar` with
//! `tools` ("Cannot use custom grammar constraints with tools") unless
//! `tool_choice` is `"none"`. Dialect models are exactly the ones that
//! machinery does not cover, so they are exactly where the proxy steps in.
//!
//! # Why `tool_choice` is rewritten to `"none"`
//!
//! That same upstream rejection is the reason the stage rewrites
//! `tool_choice` to `"none"` when it installs a grammar: it is the one
//! escape hatch llama-server leaves open for grammar + tools. The template
//! still renders the tool schemas into the prompt (templates never see
//! `tool_choice`), and the requirement the client expressed now lives in
//! the grammar itself — which is *stronger* than what `tool_choice` could
//! ask for on a model llama-server has no tool handling for anyway.
//!
//! # Why `auto` is left alone
//!
//! A grammar constrains from the first token, so under `tool_choice:
//! "auto"` it would forbid the plain-text answers `auto` exists to permit.
//! llama.cpp solves this internally with lazily-triggered grammars, but
//! does not expose lazy triggers as request fields — so `auto` keeps
//! today's behaviour: unconstrained decode, post-hoc parsing.

use serde_json::Value;
use tracing::{debug, info};

use super::ModelContext;
use crate::domain::dialect::DialectSpec;

/// Environment kill switch. Truthy values (case-insensitive `1`, `true`,
/// `yes`, `on`) disable grammar origination entirely — the same contract as
/// `GGLIB_DISABLE_MTP` and `GGLIB_DISABLE_CACHE_REUSE`.
pub const DISABLE_GRAMMAR_ENV: &str = "GGLIB_DISABLE_GRAMMAR";

/// Whether [`DISABLE_GRAMMAR_ENV`] is set to a truthy value.
fn grammar_disabled_via_env() -> bool {
    std::env::var(DISABLE_GRAMMAR_ENV).ok().is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// Originate a decode-time grammar for a demanded dialect tool call.
///
/// Engages only when *all* of the following hold, and is a no-op otherwise:
///
/// - `GGLIB_DISABLE_GRAMMAR` is not set to a truthy value;
/// - the model resolved from the catalog with a dialect spec whose codec
///   list includes JSON (see module docs for why the native path is
///   excluded — and a grammar can only originate JSON-shaped bodies);
/// - the request carries a non-empty `tools` array whose function names are
///   expressible in a GBNF literal;
/// - the client sent none of `grammar` / `json_schema` / `response_format`
///   — a client that constrains its own decode is always respected;
/// - `tool_choice` demands a call: `"required"`, or a named function.
///
/// Returns `true` when a grammar was installed.
pub fn constrain_tool_calls(body: &mut Value, ctx: &ModelContext) -> bool {
    if grammar_disabled_via_env() {
        return false;
    }
    constrain_tool_calls_inner(body, ctx)
}

/// [`constrain_tool_calls`] without the environment check, for tests.
fn constrain_tool_calls_inner(body: &mut Value, ctx: &ModelContext) -> bool {
    if !ctx.catalog_resolved {
        return false;
    }
    let Some(spec) = ctx.dialect.as_ref().filter(|s| s.supports_json_body()) else {
        return false;
    };
    if body.get("grammar").is_some()
        || body.get("json_schema").is_some()
        || body.get("response_format").is_some()
    {
        debug!("client sent its own decode constraint; not originating a grammar");
        return false;
    }

    let Some(all_names) = tool_names(body) else {
        return false;
    };

    let allowed: Vec<String> = match demanded_names(body.get("tool_choice"), &all_names) {
        Some(names) => names,
        None => return false,
    };
    if allowed.is_empty() || !allowed.iter().all(|n| gbnf_literal_safe(n)) {
        debug!("tool names not expressible in a GBNF literal; not constraining");
        return false;
    }

    let Some(grammar) = tool_call_grammar(spec, &allowed) else {
        debug!("dialect markers not expressible in a GBNF literal; not constraining");
        return false;
    };
    body["grammar"] = Value::String(grammar);
    // The one combination llama-server accepts alongside a custom grammar —
    // see the module docs. The demand now lives in the grammar.
    body["tool_choice"] = Value::String("none".into());

    info!(
        tools = allowed.len(),
        dialect = %spec.id,
        "originated decode-time grammar for a demanded dialect tool call"
    );
    true
}

/// The advertised function names, or `None` when `tools` is absent, empty,
/// or not in the `OpenAI` function-tool shape.
fn tool_names(body: &Value) -> Option<Vec<String>> {
    let tools = body.get("tools")?.as_array()?;
    if tools.is_empty() {
        return None;
    }
    let names: Vec<String> = tools
        .iter()
        .filter_map(|t| t.get("function")?.get("name")?.as_str())
        .map(str::to_owned)
        .collect();
    (names.len() == tools.len()).then_some(names)
}

/// Which names the client's `tool_choice` demands a call from.
///
/// `Some(names)` means "a call is demanded, constrain to these"; `None`
/// means "no demand" (`auto`, absent, `"none"`, or an unrecognized shape)
/// and the stage stays out of the way.
fn demanded_names(tool_choice: Option<&Value>, all_names: &[String]) -> Option<Vec<String>> {
    match tool_choice {
        Some(Value::String(s)) if s == "required" => Some(all_names.to_vec()),
        Some(Value::Object(_)) => {
            let named = tool_choice?
                .get("function")?
                .get("name")?
                .as_str()?
                .to_owned();
            // A demand for a tool that is not advertised is the client's
            // inconsistency to surface, not ours to paper over with a
            // grammar for a name the model has never seen.
            all_names.contains(&named).then(|| vec![named])
        }
        _ => None,
    }
}

/// Whether `name` can be embedded in a GBNF double-quoted literal verbatim.
///
/// Function names are identifier-like in practice; anything that would need
/// escaping (quotes, backslashes, control bytes, non-ASCII) makes the whole
/// request fall back to unconstrained decode rather than risk emitting a
/// grammar llama-server cannot compile.
fn gbnf_literal_safe(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_graphic() && c != '"' && c != '\\')
}

/// Build the GBNF grammar for one or more dialect tool calls, from the
/// same [`DialectSpec`] the parser reads.
///
/// The envelope is exactly what the [`DelimitedToolCallParser`] prefers to
/// parse — the JSON body codec, `{"name": …, "arguments": {…}}` in that
/// key order (the order the models were trained on) — wrapped in the
/// spec's markers with the spec's emission newlines. `name` is an enum of
/// the demanded tools; `arguments` is constrained to well-formed JSON.
/// Malformed envelopes, truncated JSON, and invented tool names all become
/// unrepresentable at decode time.
///
/// Returns `None` when a marker cannot be embedded in a GBNF literal
/// ([`gbnf_string_literal`]) — the caller falls back to unconstrained
/// decode rather than risk a grammar llama-server cannot compile.
///
/// [`DelimitedToolCallParser`]: crate::normalize::parsers::delimited::DelimitedToolCallParser
fn tool_call_grammar(spec: &DialectSpec, names: &[String]) -> Option<String> {
    let open = gbnf_string_literal(&spec.tool_open)?;
    let close = gbnf_string_literal(&spec.tool_close)?;
    let after_open = if spec.emission.newline_after_open {
        " nl"
    } else {
        ""
    };
    let before_close = if spec.emission.newline_before_close {
        " nl"
    } else {
        ""
    };

    let name_alternatives = names
        .iter()
        .map(|n| format!("\"\\\"{n}\\\"\""))
        .collect::<Vec<_>>()
        .join(" | ");

    Some(format!(
        r#"root ::= sp call (sp call)* sp
call ::= {open}{after_open} "{{" sp "\"name\"" sp ":" sp name sp "," sp "\"arguments\"" sp ":" sp object sp "}}"{before_close} {close}
name ::= {name_alternatives}
object ::= "{{" sp ( member ( sp "," sp member )* )? sp "}}"
member ::= string sp ":" sp value
value ::= object | array | string | number | "true" | "false" | "null"
array ::= "[" sp ( value ( sp "," sp value )* )? sp "]"
string ::= "\"" char* "\""
char ::= [^"\\\x7F\x00-\x1F] | "\\" (["\\bfnrt/] | "u" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F])
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [-+]? [0-9]+)?
sp ::= [ \t\r\n]*
nl ::= "\n"
"#
    ))
}

/// Render `s` as a GBNF double-quoted literal, escaping `"` and `\`.
///
/// Returns `None` for an empty string or one containing control bytes —
/// markers a grammar cannot express verbatim make the caller fall back to
/// unconstrained decode.
fn gbnf_string_literal(s: &str) -> Option<String> {
    if s.is_empty() || s.chars().any(char::is_control) {
        return None;
    }
    let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
    Some(format!("\"{escaped}\""))
}

#[cfg(test)]
mod tests {
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
            let grammar = tool_call_grammar(&spec, &["read_file".to_owned()]).unwrap();
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
        let grammar = tool_call_grammar(&quoted, &["f".to_owned()]).unwrap();
        assert!(grammar.contains(r#""<\"q\">""#));

        let control = DialectSpec {
            tool_open: "<a\u{1}b>".to_owned(),
            ..DialectSpec::qwen_xml()
        };
        assert_eq!(tool_call_grammar(&control, &["f".to_owned()]), None);

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
        let grammar = tool_call_grammar(&no_newlines, &["f".to_owned()]).unwrap();
        assert!(grammar.contains(r#"call ::= "<tool_call>" "{""#));

        let with_newlines = tool_call_grammar(&DialectSpec::qwen_xml(), &["f".to_owned()]).unwrap();
        assert!(with_newlines.contains(r#"call ::= "<tool_call>" nl "{""#));
    }
}
