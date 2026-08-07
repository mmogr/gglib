//! Template render-and-diff dialect derivation.
//!
//! A chat template does not only render *input* — it also renders prior
//! assistant turns that contain `tool_calls`, and that rendering is
//! precisely the markup the model was trained to emit.  This module
//! executes the GGUF's own `tokenizer.chat_template` against two probe
//! conversations that differ only in the final assistant turn — plain
//! sentinel text vs. a sentinel tool call — and diffs the two renders to
//! extract the tool-call envelope markers, producing a
//! [`DialectSpec`] with no per-family code.
//!
//! ## The conservative rule
//!
//! A spec is only emitted when the rendered payload round-trips through
//! the JSON body codec (`{"name": …, "arguments": …}`).  Dialects whose
//! bodies are not JSON-decodable — `DeepSeek`'s fenced blocks, Llama 3's
//! `"parameters"` key — yield `None` and fall back to the pattern-table
//! path exactly as before: a wrong spec would corrupt client output, while
//! a missing spec merely preserves today's behaviour.
//!
//! ## Canonicalization
//!
//! When the derived markers equal the built-in Qwen spec's, the builtin is
//! returned instead of the derived spec.  The probe can only observe the
//! JSON codec (templates render JSON), but Qwen 3 models *emit* the
//! inner-XML body at runtime under `--jinja` — the builtin carries that
//! fallback codec, plus the `call_qwen_` ID prefix continuity.
//!
//! ## Failure is normal
//!
//! Everything here is best-effort: any compile error, render error, diff
//! anomaly, or validation failure returns `None` with a `tracing::debug!`
//! reason, and detection proceeds on the pattern tables.  Quantized GGUFs
//! with stripped or exotic templates are an expected input, not an error.

use std::collections::HashMap;

use gglib_core::domain::dialect::{
    BodyCodec, DERIVED_DIALECT_ID, DERIVED_ID_PREFIX, DialectSpec, EmissionProfile,
};
use minijinja::{Environment, UndefinedBehavior, context};
use serde_json::{Value, json};
use tracing::debug;

/// Sentinel content for the plain-text assistant turn.  The snowmen pin
/// the diff: no real template scaffolding starts or ends with one, so the
/// common prefix/suffix cannot bite into the payload.
const PLAIN: &str = "\u{2603}GGLIB-PLAIN-SENTINEL\u{2603}";

/// Sentinel function name for the tool-call assistant turn.
const FN_NAME: &str = "gglib_probe_fn";

/// Sentinel argument value inside the probe call's `arguments`.
const ARG_VAL: &str = "\u{2603}GGLIB-ARG-SENTINEL\u{2603}";

/// Upper bound on a plausible marker, in bytes.  Anything longer is a
/// mis-split (the diff bit into per-model prose, not a marker).
const MAX_MARKER_BYTES: usize = 64;

/// Derive a [`DialectSpec`] from the model's chat template, if possible.
///
/// See the module docs for the algorithm and the conservative rule.
pub(super) fn derive(metadata: &HashMap<String, String>) -> Option<DialectSpec> {
    let template = metadata.get("tokenizer.chat_template")?;
    if template.trim().is_empty() {
        debug!("template probe: chat template empty");
        return None;
    }

    let env = match build_env(template) {
        Ok(env) => env,
        Err(e) => {
            debug!("template probe: template failed to compile: {e}");
            return None;
        }
    };

    // Attempt order: (i) the HF nested shape with `arguments` as a mapping
    // (what `| tojson` templates expect), then (ii) `arguments`
    // pre-serialized as a JSON string (templates that print it raw).
    [false, true]
        .into_iter()
        .find_map(|args_as_string| try_derive(&env, args_as_string))
}

/// Build the lenient probe environment for `template`.
fn build_env(template: &str) -> Result<Environment<'_>, minijinja::Error> {
    let mut env = Environment::new();
    // Missing variables render as empty rather than erroring — probe
    // contexts cannot anticipate every variable a template consults.
    env.set_undefined_behavior(UndefinedBehavior::Lenient);
    // Python-isms (`.strip()`, `.split()`, …) that HF templates use freely.
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    // HF templates abort unsupported inputs through `raise_exception`.
    env.add_function(
        "raise_exception",
        |msg: String| -> Result<minijinja::Value, minijinja::Error> {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                msg,
            ))
        },
    );
    // Deterministic stub: the date cancels in the A/B diff as long as both
    // renders agree, and a fixed value guarantees they do.
    env.add_function("strftime_now", |_format: String| String::from("2024-01-01"));
    env.add_template("probe", template)?;
    Ok(env)
}

/// One full derivation attempt with the chosen `arguments` encoding.
fn try_derive(env: &Environment<'_>, args_as_string: bool) -> Option<DialectSpec> {
    let tools = probe_tools();
    let a = render(env, &conversation(&assistant_plain()), &tools)?;
    let b = render(
        env,
        &conversation(&assistant_tool_call(args_as_string)?),
        &tools,
    )?;
    if a.is_empty() || b.is_empty() || a == b {
        debug!("template probe: renders empty or identical (template ignores tool_calls)");
        return None;
    }

    let (a_mid, b_mid) = diff_middles(&a, &b);
    // The divergence must be the assistant turn itself: A's middle is the
    // plain sentinel, B's contains the probe call and no plain sentinel.
    if !a_mid.contains(PLAIN) || b_mid.contains(PLAIN) || !b_mid.contains(FN_NAME) {
        debug!("template probe: diff did not isolate the assistant turn");
        return None;
    }

    let (payload_start, payload_end) = find_payload(b_mid)?;
    let raw_open = &b_mid[..payload_start];
    let raw_close = &b_mid[payload_end..];

    let tool_open = raw_open.trim();
    let newline_after_open = raw_open[raw_open.len() - trailing_ws_len(raw_open)..].contains('\n');
    let tool_close = raw_close.trim();
    let newline_before_close = raw_close[..leading_ws_len(raw_close)].contains('\n');

    if !valid_marker(tool_open) || !valid_marker(tool_close) {
        debug!(open = %tool_open, close = %tool_close, "template probe: implausible markers");
        return None;
    }

    // Canonicalize to the audited builtin when the markers match it — see
    // the module docs for why the builtin (with its inner-XML fallback
    // codec) must win over a freshly-derived JSON-only spec.
    let builtin = DialectSpec::qwen_xml();
    if tool_open == builtin.tool_open && tool_close == builtin.tool_close {
        return Some(builtin);
    }

    Some(DialectSpec {
        id: DERIVED_DIALECT_ID.to_owned(),
        tool_open: tool_open.to_owned(),
        tool_close: tool_close.to_owned(),
        body_codecs: vec![BodyCodec::Json],
        emission: EmissionProfile {
            newline_after_open,
            newline_before_close,
        },
        id_prefix: DERIVED_ID_PREFIX.to_owned(),
    })
}

/// The advertised probe tool, in the `OpenAI` function shape templates
/// iterate over.  Identical in both renders, so it cancels in the diff.
fn probe_tools() -> Value {
    json!([{
        "type": "function",
        "function": {
            "name": FN_NAME,
            "description": "gglib dialect probe",
            "parameters": {
                "type": "object",
                "properties": { "probe_arg": { "type": "string" } },
                "required": ["probe_arg"],
            },
        },
    }])
}

/// The three-message probe conversation around `final_assistant`.
fn conversation(final_assistant: &Value) -> Value {
    json!([
        { "role": "system", "content": "You are a helpful assistant." },
        { "role": "user", "content": "hi" },
        final_assistant,
    ])
}

fn assistant_plain() -> Value {
    json!({ "role": "assistant", "content": PLAIN })
}

fn assistant_tool_call(args_as_string: bool) -> Option<Value> {
    let args = json!({ "probe_arg": ARG_VAL });
    let arguments = if args_as_string {
        Value::String(serde_json::to_string(&args).ok()?)
    } else {
        args
    };
    Some(json!({
        "role": "assistant",
        "content": "",
        "tool_calls": [{
            "id": "call_probe_0",
            "type": "function",
            "function": { "name": FN_NAME, "arguments": arguments },
        }],
    }))
}

/// Render the probe template with a conversation and the probe tool list.
fn render(env: &Environment<'_>, messages: &Value, tools: &Value) -> Option<String> {
    let tmpl = env.get_template("probe").ok()?;
    tmpl.render(context! {
        messages => minijinja::Value::from_serialize(messages),
        tools => minijinja::Value::from_serialize(tools),
        add_generation_prompt => false,
        bos_token => "",
        eos_token => "",
    })
    .map_err(|e| debug!("template probe: render failed: {e}"))
    .ok()
}

/// Strip the longest common prefix and suffix shared by `a` and `b`,
/// returning the differing middles.  Both cuts respect char boundaries in
/// both strings and never overlap.
fn diff_middles<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
    let mut p = a
        .as_bytes()
        .iter()
        .zip(b.as_bytes())
        .take_while(|(x, y)| x == y)
        .count();
    while p > 0 && !(a.is_char_boundary(p) && b.is_char_boundary(p)) {
        p -= 1;
    }
    let (ra, rb) = (&a[p..], &b[p..]);

    let mut s = ra
        .as_bytes()
        .iter()
        .rev()
        .zip(rb.as_bytes().iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    while s > 0 && !(ra.is_char_boundary(ra.len() - s) && rb.is_char_boundary(rb.len() - s)) {
        s -= 1;
    }
    (&ra[..ra.len() - s], &rb[..rb.len() - s])
}

/// Locate the probe payload in `text`: the first balanced `{…}` span that
/// parses as JSON carrying the sentinel name and argument.  This is the
/// conservative rule — no JSON payload, no spec.
fn find_payload(text: &str) -> Option<(usize, usize)> {
    for (start, ch) in text.char_indices() {
        if ch != '{' {
            continue;
        }
        let Some(len) = balanced_object_len(&text[start..]) else {
            continue;
        };
        if let Ok(Value::Object(obj)) = serde_json::from_str(&text[start..start + len])
            && obj.get("name").and_then(Value::as_str) == Some(FN_NAME)
            && arguments_match(obj.get("arguments"))
        {
            return Some((start, start + len));
        }
    }
    debug!("template probe: no JSON payload round-tripped (non-JSON body dialect)");
    None
}

/// Byte length of the balanced `{…}` object starting at `s[0]`, tracking
/// JSON string context so braces inside string values don't count.
fn balanced_object_len(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + c.len_utf8());
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether the payload's `arguments` carries the sentinel — either as a
/// mapping or as a nested JSON string (both real template behaviours).
fn arguments_match(arguments: Option<&Value>) -> bool {
    match arguments {
        Some(Value::Object(args)) => args.get("probe_arg").and_then(Value::as_str) == Some(ARG_VAL),
        Some(Value::String(s)) => serde_json::from_str::<Value>(s)
            .is_ok_and(|v| v.get("probe_arg").and_then(Value::as_str) == Some(ARG_VAL)),
        _ => false,
    }
}

/// A plausible envelope marker: non-empty, bounded, no braces (a brace
/// means the diff bit into JSON), and no sentinel residue.
fn valid_marker(marker: &str) -> bool {
    !marker.is_empty()
        && marker.len() <= MAX_MARKER_BYTES
        && !marker.contains(['{', '}'])
        && !marker.contains(FN_NAME)
        && !marker.contains(ARG_VAL)
        && !marker.contains(PLAIN)
}

/// Byte length of the trailing whitespace of `s`.
fn trailing_ws_len(s: &str) -> usize {
    s.len() - s.trim_end().len()
}

/// Byte length of the leading whitespace of `s`.
fn leading_ws_len(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture templates modeled on the upstream chat templates of each
    /// family — reduced to the message loop and tool-call emission logic
    /// the probe exercises, with the exact markup shapes the real
    /// templates render.
    const QWEN2_5: &str = include_str!("testdata/qwen2_5.jinja");
    const QWEN3: &str = include_str!("testdata/qwen3.jinja");
    const HERMES_2_PRO: &str = include_str!("testdata/hermes2_pro.jinja");
    const LLAMA3_1: &str = include_str!("testdata/llama3_1.jinja");
    const DEEPSEEK_R1: &str = include_str!("testdata/deepseek_r1.jinja");

    fn meta(template: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("tokenizer.chat_template".to_owned(), template.to_owned());
        m
    }

    #[test]
    fn qwen2_5_template_derives_the_builtin_spec() {
        let spec = derive(&meta(QWEN2_5)).expect("qwen2.5 template must derive");
        assert_eq!(spec, DialectSpec::qwen_xml());
        // The canonicalization guarantee in one assertion: the runtime
        // inner-XML fallback codec must survive derivation.
        assert!(spec.body_codecs.contains(&BodyCodec::FunctionXml));
    }

    #[test]
    fn qwen3_template_derives_the_builtin_spec() {
        let spec = derive(&meta(QWEN3)).expect("qwen3 template must derive");
        assert_eq!(spec, DialectSpec::qwen_xml());
    }

    /// The agnosticism proof: a non-Qwen model whose template renders the
    /// same envelope derives the same spec, with zero Hermes-specific code
    /// and no model-name sniffing.
    #[test]
    fn hermes_template_derives_the_builtin_spec() {
        let spec = derive(&meta(HERMES_2_PRO)).expect("hermes template must derive");
        assert_eq!(spec, DialectSpec::qwen_xml());
    }

    /// Llama 3.1 renders bare JSON with a `"parameters"` key — no
    /// envelope, wrong key. The conservative rule must refuse it.
    #[test]
    fn llama3_1_template_yields_none() {
        assert_eq!(derive(&meta(LLAMA3_1)), None);
    }

    /// `DeepSeek` R1's body is a fenced block, not bare JSON with
    /// name/arguments. The conservative rule must refuse it.
    #[test]
    fn deepseek_r1_template_yields_none() {
        assert_eq!(derive(&meta(DEEPSEEK_R1)), None);
    }

    #[test]
    fn missing_or_empty_template_yields_none() {
        assert_eq!(derive(&HashMap::new()), None);
        assert_eq!(derive(&meta("   ")), None);
    }

    #[test]
    fn uncompilable_template_yields_none() {
        assert_eq!(derive(&meta("{% invalid syntax")), None);
    }

    #[test]
    fn template_that_ignores_tool_calls_yields_none() {
        // Renders every message's content only — A and B differ, but B has
        // no payload (content is empty in B), so the diff sanity fails.
        let t = "{% for m in messages %}[{{ m.role }}] {{ m.content }}\n{% endfor %}";
        assert_eq!(derive(&meta(t)), None);
    }

    /// A template family the codebase has never seen: custom multibyte
    /// markers derive a working spec with no code changes — the point of
    /// the whole mechanism.
    #[test]
    fn custom_marker_template_derives_a_spec() {
        let t = concat!(
            "{% for m in messages %}",
            "<msg:{{ m.role }}>",
            "{% if m.tool_calls %}",
            "{% for tc in m.tool_calls %}",
            "«TC»\n{\"name\": \"{{ tc.function.name }}\", \"arguments\": {{ tc.function.arguments | tojson }}}\n«/TC»",
            "{% endfor %}",
            "{% else %}{{ m.content }}{% endif %}",
            "</msg>\n",
            "{% endfor %}",
        );
        let spec = derive(&meta(t)).expect("custom markers must derive");
        assert_eq!(spec.id, DERIVED_DIALECT_ID);
        assert_eq!(spec.tool_open, "«TC»");
        assert_eq!(spec.tool_close, "«/TC»");
        assert_eq!(spec.body_codecs, vec![BodyCodec::Json]);
        assert_eq!(spec.id_prefix, DERIVED_ID_PREFIX);
        assert!(spec.emission.newline_after_open);
        assert!(spec.emission.newline_before_close);
    }

    /// Fenced dialects — open marker equals close marker — are valid.
    #[test]
    fn identical_open_and_close_markers_derive() {
        let t = concat!(
            "{% for m in messages %}",
            "{% if m.tool_calls %}",
            "{% for tc in m.tool_calls %}",
            "@@TOOL@@{\"name\": \"{{ tc.function.name }}\", \"arguments\": {{ tc.function.arguments | tojson }}}@@TOOL@@",
            "{% endfor %}",
            "{% else %}{{ m.content }}{% endif %}",
            "{% endfor %}",
        );
        let spec = derive(&meta(t)).expect("fenced markers must derive");
        assert_eq!(spec.tool_open, "@@TOOL@@");
        assert_eq!(spec.tool_close, "@@TOOL@@");
        assert!(!spec.emission.newline_after_open);
        assert!(!spec.emission.newline_before_close);
    }

    /// Templates that print `arguments` raw (no `tojson`) are covered by
    /// the second attempt, which passes it pre-serialized.
    #[test]
    fn raw_arguments_interpolation_derives_via_the_string_attempt() {
        let t = concat!(
            "{% for m in messages %}",
            "{% if m.tool_calls %}",
            "{% for tc in m.tool_calls %}",
            "[CALL]{\"name\": \"{{ tc.function.name }}\", \"arguments\": {{ tc.function.arguments }}}[/CALL]",
            "{% endfor %}",
            "{% else %}{{ m.content }}{% endif %}",
            "{% endfor %}",
        );
        let spec = derive(&meta(t)).expect("raw interpolation must derive");
        assert_eq!(spec.tool_open, "[CALL]");
        assert_eq!(spec.tool_close, "[/CALL]");
    }

    #[test]
    fn diff_middles_respects_multibyte_boundaries() {
        let (a, b) = diff_middles("x☃ay", "x☃by");
        assert_eq!(a, "a");
        assert_eq!(b, "b");
        // Shared multibyte char adjacent to the difference.
        let (a, b) = diff_middles("pre☃apost", "pre☃bpost");
        assert_eq!(a, "a");
        assert_eq!(b, "b");
    }

    #[test]
    fn balanced_object_len_is_string_aware() {
        let s = r#"{"a": "}", "b": {"c": 1}} tail"#;
        let len = balanced_object_len(s).unwrap();
        assert_eq!(&s[..len], r#"{"a": "}", "b": {"c": 1}}"#);
    }
}
