//! Spec-driven delimited tool-call parser.
//!
//! **Tier A — Compensation** ([ADR 0001]). This parser exists because
//! llama-server handed dialect tool calls to the client as raw text. It is
//! not gglib's job in principle; it is gglib's job until upstream does it
//! correctly.
//!
//! *Deletion criterion:* llama.cpp's `peg-native` parser handles the
//! delimited dialects gglib tags, for every tagged model, **and** the
//! failure modes this parser was hardened against no longer reproduce —
//! specifically a parameter value containing a literal `</parameter>`
//! ([#24807]) and a reasoning model emitting prose before the open marker
//! ([#20260]). Evidence is the drift alarm ([`crate::normalize::residue`])
//! reporting zero residue across a release cycle with this parser bypassed,
//! not the mere presence of [`RuntimeFlags::PEG_NATIVE_TOOL_CALLS`].
//!
//! [ADR 0001]: https://github.com/mmogr/gglib/blob/main/docs/adr/0001-runtime-capability-tiers.md
//! [`RuntimeFlags::PEG_NATIVE_TOOL_CALLS`]: crate::domain::RuntimeFlags::PEG_NATIVE_TOOL_CALLS
//! [#24807]: https://github.com/ggml-org/llama.cpp/issues/24807
//! [#20260]: https://github.com/ggml-org/llama.cpp/issues/20260
//!
//! Rewrites `OPEN...CLOSE` tool-call markup — emitted inside the text or
//! reasoning channel — into proper [`ToolCall`] values, where the envelope
//! markers come from a [`DialectSpec`] rather than being hardcoded.  Bytes
//! outside envelope regions are forwarded verbatim on the channel they
//! arrived on.  The built-in [`DialectSpec::qwen_xml`] spec reproduces the
//! historical Qwen 2 / 2.5 / 3 behaviour (`<tool_call>` markers); template-
//! derived specs drive the exact same machine with their own markers.
//!
//! Body decoding is selected by the spec's ordered [`BodyCodec`] list:
//! 1. [`BodyCodec::Json`] — `{"name":"foo","arguments":{...}}`
//!    (Qwen 2 / 2.5, Hermes, and template-derived dialects).
//! 2. [`BodyCodec::FunctionXml`] —
//!    `<function=NAME><parameter=KEY>VALUE</parameter>...</function>`,
//!    one or more back-to-back inside a single wrapper (Qwen 3 + `--jinja`,
//!    Hermes-style).  The inner markers are codec-internal constants,
//!    invariant across models that use the codec.
//!
//! ## Chunk safety
//!
//! Either marker may straddle SSE chunk boundaries.  The parser holds back
//! at most `marker.len() - 1` bytes per channel as a lookahead buffer.  The
//! buffered bytes are flushed on the next push or at
//! [`ToolCallParser::finish`].
//!
//! ## Cross-channel handling
//!
//! In practice a tool call appears entirely on one channel — either text
//! (no reasoning split) or reasoning (when `--reasoning-format` is on).
//! Each channel therefore maintains its own independent parser state
//! ([`ChannelState`]) so that markup never crosses channels.  The synthesised
//! tool-call IDs share a single monotonic counter across both channels.

use serde_json::Value;

use super::super::error::NormalizationError;
use super::super::parser::{ParserOutput, ToolCallParser};
use crate::domain::agent::ToolCall;
use crate::domain::dialect::{BodyCodec, DialectSpec};

/// Per-channel scanning state.  The text and reasoning channels each own
/// one of these; they never share buffers.
#[derive(Default, Debug)]
struct ChannelState {
    /// Trailing bytes whose status (markup vs payload) is not yet decided.
    pending: String,
    /// `true` between an open and close marker.
    inside: bool,
    /// JSON body accumulated while `inside` is true.
    body: String,
}

/// Output channel selector — keeps `scan` channel-agnostic.
#[derive(Copy, Clone)]
enum Channel {
    Text,
    Reasoning,
}

/// Parser for delimited tool-call dialects, configured by a
/// [`DialectSpec`].  See module docs.
#[derive(Debug)]
pub struct DelimitedToolCallParser {
    /// The dialect being parsed: markers, body codecs, and ID prefix.
    spec: DialectSpec,
    text: ChannelState,
    reasoning: ChannelState,
    /// Monotonic counter for synthesised tool-call IDs.  Shared across
    /// both channels so IDs remain globally unique within a single stream.
    next_id: u32,
}

impl DelimitedToolCallParser {
    /// Construct a fresh parser for `spec` with empty per-channel buffers.
    #[must_use]
    pub fn new(spec: DialectSpec) -> Self {
        Self {
            spec,
            text: ChannelState::default(),
            reasoning: ChannelState::default(),
            next_id: 0,
        }
    }

    /// Mint a stream-unique synthetic ID for an extracted tool call.
    fn mint_id(&mut self) -> String {
        let n = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        format!("{}{n}", self.spec.id_prefix)
    }

    /// Drive the state machine for one channel.
    ///
    /// All scanning logic lives here; `push_text` and `push_reasoning` are
    /// thin dispatch wrappers that pick the right `ChannelState` and route
    /// flushed bytes to the right output field.
    fn scan(&mut self, channel: Channel, chunk: &str) -> ParserOutput {
        let mut out = ParserOutput::default();

        // Clone the spec's marker/codec data into locals so the loop below
        // can keep borrowing `&mut self` for `mint_id`.  The strings are a
        // few bytes each; one clone per chunk is noise next to the scan.
        let open = self.spec.tool_open.clone();
        let close = self.spec.tool_close.clone();
        let codecs = self.spec.body_codecs.clone();

        // Take ownership of the channel state by moving it out, then put it
        // back at the end.  This sidesteps the borrow conflict between
        // `&mut self.text` (or `.reasoning`) and `&mut self` for `mint_id`.
        let mut state = match channel {
            Channel::Text => std::mem::take(&mut self.text),
            Channel::Reasoning => std::mem::take(&mut self.reasoning),
        };

        state.pending.push_str(chunk);

        loop {
            if state.inside {
                if let Some(p) = state.pending.find(&*close) {
                    state.body.push_str(&state.pending[..p]);
                    finalize_tool_call(&codecs, &state.body, &mut out, || self.mint_id());
                    state.body.clear();
                    state.inside = false;
                    state.pending.drain(..p + close.len());
                    continue;
                }
                let keep = partial_suffix_len(state.pending.as_bytes(), close.as_bytes());
                let flush_to = state.pending.len() - keep;
                state.body.push_str(&state.pending[..flush_to]);
                state.pending.drain(..flush_to);
                break;
            }

            // Outside any tool-call envelope.
            if let Some(p) = state.pending.find(&*open) {
                forward(&mut out, channel, &state.pending[..p]);
                state.pending.drain(..p + open.len());
                state.inside = true;
                continue;
            }
            let keep = partial_suffix_len(state.pending.as_bytes(), open.as_bytes());
            let flush_to = state.pending.len() - keep;
            forward(&mut out, channel, &state.pending[..flush_to]);
            state.pending.drain(..flush_to);
            break;
        }

        match channel {
            Channel::Text => self.text = state,
            Channel::Reasoning => self.reasoning = state,
        }
        out
    }

    /// Flush a single channel at end-of-stream.
    fn flush_channel(&mut self, channel: Channel) -> ParserOutput {
        let mut out = ParserOutput::default();
        let state = match channel {
            Channel::Text => std::mem::take(&mut self.text),
            Channel::Reasoning => std::mem::take(&mut self.reasoning),
        };
        if state.inside {
            // Stream ended mid-`<tool_call>`.  Surface as an error and
            // discard the partial body — we have no way to know how it
            // would have closed.
            let mut partial = state.body;
            partial.push_str(&state.pending);
            out.errors
                .push(NormalizationError::unclosed_tool_call(partial));
        } else {
            // Any held-back bytes turned out to be ordinary text — flush.
            forward(&mut out, channel, &state.pending);
        }
        out
    }
}

impl ToolCallParser for DelimitedToolCallParser {
    fn push_text(&mut self, chunk: &str) -> ParserOutput {
        self.scan(Channel::Text, chunk)
    }

    fn push_reasoning(&mut self, chunk: &str) -> ParserOutput {
        self.scan(Channel::Reasoning, chunk)
    }

    fn finish(&mut self) -> ParserOutput {
        let mut a = self.flush_channel(Channel::Text);
        let b = self.flush_channel(Channel::Reasoning);
        a.forward_text.push_str(&b.forward_text);
        a.forward_reasoning.push_str(&b.forward_reasoning);
        a.tool_calls.extend(b.tool_calls);
        a.errors.extend(b.errors);
        a
    }
}

// =============================================================================
// Free helpers
// =============================================================================

/// Append `bytes` to the channel-appropriate field of `out`.
fn forward(out: &mut ParserOutput, channel: Channel, bytes: &str) {
    if bytes.is_empty() {
        return;
    }
    match channel {
        Channel::Text => out.forward_text.push_str(bytes),
        Channel::Reasoning => out.forward_reasoning.push_str(bytes),
    }
}

/// Parse the accumulated tool-call body and push the resulting [`ToolCall`]s
/// (or a [`NormalizationError`]) onto `out`.
///
/// The spec's `codecs` are tried in order; the first codec that decodes the
/// body wins.  The built-in Qwen spec lists JSON before inner XML: JSON is
/// the historical Qwen format and is unambiguous, while the XML form is the
/// documented fallback for Qwen3 chat templates that emit nested
/// function/parameter markup inside the envelope.
///
/// On failure, the error kind reflects which codec was plausibly in play: a
/// body that looks like it opened the inner-XML codec (`<function=`) — when
/// that codec is enabled — reports
/// [`NormalizationErrorKind::MalformedFunctionXml`] instead of the generic
/// JSON failure, since the two codecs fail for unrelated reasons and a log
/// reader should not have to guess which one was attempted.
///
/// [`NormalizationErrorKind::MalformedFunctionXml`]: crate::normalize::error::NormalizationErrorKind::MalformedFunctionXml
fn finalize_tool_call(
    codecs: &[BodyCodec],
    body: &str,
    out: &mut ParserOutput,
    mut mint_id: impl FnMut() -> String,
) {
    let trimmed = body.trim();
    for codec in codecs {
        match codec {
            BodyCodec::Json => {
                if let Some(call) = parse_json_body(trimmed, &mut mint_id) {
                    out.tool_calls.push(call);
                    return;
                }
            }
            BodyCodec::FunctionXml => {
                if let Some(calls) = parse_function_xml_body(trimmed, &mut mint_id) {
                    out.tool_calls.extend(calls);
                    return;
                }
            }
        }
    }
    let error = if codecs.contains(&BodyCodec::FunctionXml) && trimmed.starts_with("<function=") {
        NormalizationError::malformed_function_xml(body.to_owned())
    } else {
        NormalizationError::malformed_tool_call(body.to_owned())
    };
    out.errors.push(error);
}

/// Try to interpret `body` as a Qwen JSON tool call.
fn parse_json_body(body: &str, mint_id: &mut impl FnMut() -> String) -> Option<ToolCall> {
    let parsed: Value = serde_json::from_str(body).ok()?;
    let obj = parsed.as_object()?;
    let name = obj.get("name").and_then(Value::as_str)?.to_owned();
    let arguments = obj
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    Some(ToolCall {
        id: mint_id(),
        name,
        arguments,
    })
}

/// Try to interpret `body` as one or more back-to-back Hermes/Qwen3
/// inner-XML tool calls:
/// `<function=NAME><parameter=KEY>VALUE</parameter>...</function>`, repeated.
///
/// Whitespace between tags is tolerated. Each parameter value is parsed as
/// JSON when it looks like a JSON literal (`{`, `[`, quoted string, number,
/// `true`/`false`/`null`); otherwise it is forwarded as a string after
/// trimming surrounding whitespace — see [`parse_param_value`]'s doc comment
/// for the coercion's known limitation.
///
/// Returns `None` (not `Some(vec![])`) when `body` doesn't open with
/// `<function=` at all or the very first block is malformed, so the caller
/// can fall through to a "no dialect matched" error. A body that opens
/// correctly but has a malformed *later* block currently stops at that point
/// and returns `None` for the whole body, discarding any calls already
/// parsed — the same fail-shut behaviour the single-call parser always had.
fn parse_function_xml_body(
    body: &str,
    mint_id: &mut impl FnMut() -> String,
) -> Option<Vec<ToolCall>> {
    let mut calls = Vec::new();
    let mut cursor = body.trim();

    while !cursor.is_empty() {
        let after_open = cursor.strip_prefix("<function=")?;
        let name_end = after_open.find('>')?;
        let name = after_open[..name_end].trim();
        if name.is_empty() {
            return None;
        }
        let after_name = &after_open[name_end + 1..];

        // This block's own `</function>` is the LAST occurrence before the
        // next sibling `<function=`, if any — never the first occurrence
        // found anywhere in the remainder, which could belong to a
        // parameter's own value (e.g. a `content` parameter whose text
        // happens to mention "</function>"). See `find_own_close` for the
        // same rule applied to `</parameter>`.
        let close_at = find_own_close(after_name, "</function>", "<function=")?;
        let inner = after_name[..close_at].trim();
        let after_function = &after_name[close_at + "</function>".len()..];

        let mut args = serde_json::Map::new();
        let mut param_cursor = inner;
        while !param_cursor.is_empty() {
            param_cursor = param_cursor.trim_start();
            if param_cursor.is_empty() {
                break;
            }
            let after_param = param_cursor.strip_prefix("<parameter=")?;
            let key_end = after_param.find('>')?;
            let key = after_param[..key_end].trim().to_owned();
            if key.is_empty() {
                return None;
            }
            let rest = &after_param[key_end + 1..];
            let close_at = find_own_close(rest, "</parameter>", "<parameter=")?;
            let raw_value = rest[..close_at].trim();
            args.insert(key, parse_param_value(raw_value));
            param_cursor = &rest[close_at + "</parameter>".len()..];
        }

        calls.push(ToolCall {
            id: mint_id(),
            name: name.to_owned(),
            arguments: Value::Object(args),
        });

        cursor = after_function.trim_start();
    }

    (!calls.is_empty()).then_some(calls)
}

/// Find this tag's own closing marker inside `rest`: the LAST occurrence of
/// `close` before the next sibling `next_open` marker (or before the end of
/// `rest`, if there is no next sibling).
///
/// A naive `rest.find(close)` truncates the value early whenever it happens
/// to contain the literal closing-tag text — a real risk for a `content` or
/// `code` parameter carrying anything that looks like markup. The tag's true
/// close is always the one immediately before its next sibling opens (or the
/// end of the block), never an earlier occurrence, so searching backward
/// from that boundary finds it correctly even when the value embeds the
/// marker text. This is not a complete fix — a value that also happens to
/// contain the *next sibling's* open marker is still ambiguous, since this
/// dialect has no escaping mechanism — but it is strictly more often correct
/// than a forward search from the start.
fn find_own_close(rest: &str, close: &str, next_open: &str) -> Option<usize> {
    let boundary = rest.find(next_open).unwrap_or(rest.len());
    rest[..boundary].rfind(close)
}

/// Best-effort coercion of a `<parameter>` body to a JSON value. Falls back
/// to a string literal when the body is not valid JSON.
///
/// This is inherently lossy: the dialect gives no way to distinguish a
/// parameter that is genuinely meant to be the *string* `"true"` or `"123"`
/// from one meant to be the boolean or the number — both coerce to the typed
/// value. There is no tool `input_schema` available here to disambiguate
/// against (the parser has no access to the tool definitions that produced
/// this call), so this is a best-effort guess, not a guarantee.
fn parse_param_value(raw: &str) -> Value {
    if raw.is_empty() {
        return Value::String(String::new());
    }
    if let Ok(v) = serde_json::from_str::<Value>(raw) {
        return v;
    }
    Value::String(raw.to_owned())
}

/// Largest `n` in `[0, marker.len())` such that the last `n` bytes of `buf`
/// are a prefix of `marker`.  Used as the lookahead window for chunk-safe
/// marker detection.
fn partial_suffix_len(buf: &[u8], marker: &[u8]) -> usize {
    if marker.len() < 2 {
        return 0;
    }
    let max = std::cmp::min(buf.len(), marker.len() - 1);
    for n in (1..=max).rev() {
        if buf.ends_with(&marker[..n]) {
            return n;
        }
    }
    0
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A parser wired with the built-in Qwen spec — the configuration every
    /// pre-spec test in this module was written against.
    fn qwen() -> DelimitedToolCallParser {
        DelimitedToolCallParser::new(DialectSpec::qwen_xml())
    }

    fn collect(p: &mut DelimitedToolCallParser, chunks: &[&str]) -> ParserOutput {
        let mut total = ParserOutput::default();
        for c in chunks {
            let o = p.push_text(c);
            total.forward_text.push_str(&o.forward_text);
            total.forward_reasoning.push_str(&o.forward_reasoning);
            total.tool_calls.extend(o.tool_calls);
            total.errors.extend(o.errors);
        }
        let f = p.finish();
        total.forward_text.push_str(&f.forward_text);
        total.forward_reasoning.push_str(&f.forward_reasoning);
        total.tool_calls.extend(f.tool_calls);
        total.errors.extend(f.errors);
        total
    }

    #[test]
    fn passthrough_with_no_markup() {
        let mut p = qwen();
        let out = collect(&mut p, &["hello ", "world"]);
        assert_eq!(out.forward_text, "hello world");
        assert!(out.tool_calls.is_empty());
        assert!(out.errors.is_empty());
    }

    #[test]
    fn extracts_simple_tool_call_from_text() {
        let mut p = qwen();
        let out = collect(
            &mut p,
            &[r#"before<tool_call>{"name":"foo","arguments":{"x":1}}</tool_call>after"#],
        );
        assert_eq!(out.forward_text, "beforeafter");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].id, "call_qwen_0");
        assert_eq!(out.tool_calls[0].name, "foo");
        assert_eq!(out.tool_calls[0].arguments, json!({"x": 1}));
        assert!(out.errors.is_empty());
    }

    #[test]
    fn open_tag_straddles_chunk_boundary() {
        let mut p = qwen();
        let out = collect(
            &mut p,
            &[
                "before<tool",
                "_call>",
                r#"{"name":"foo","arguments":{}}"#,
                "</tool_call>",
                "after",
            ],
        );
        assert_eq!(out.forward_text, "beforeafter");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "foo");
    }

    #[test]
    fn close_tag_straddles_chunk_boundary() {
        let mut p = qwen();
        let out = collect(
            &mut p,
            &[
                "<tool_call>",
                r#"{"name":"foo","arguments":{}}</tool"#,
                "_call>tail",
            ],
        );
        assert_eq!(out.forward_text, "tail");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "foo");
    }

    #[test]
    fn one_byte_at_a_time_still_works() {
        let mut p = qwen();
        let s = r#"x<tool_call>{"name":"f","arguments":{"a":2}}</tool_call>y"#;
        let chunks: Vec<String> = s.chars().map(|c| c.to_string()).collect();
        let refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        let out = collect(&mut p, &refs);
        assert_eq!(out.forward_text, "xy");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].arguments, json!({"a": 2}));
    }

    #[test]
    fn tool_call_in_reasoning_channel_is_extracted() {
        let mut p = qwen();
        let chunk = r#"thinking <tool_call>{"name":"foo","arguments":{}}</tool_call> done"#;
        let out = p.push_reasoning(chunk);
        let f = p.finish();
        assert_eq!(out.forward_reasoning, "thinking  done");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "foo");
        assert!(f.is_empty());
    }

    #[test]
    fn malformed_json_emits_error() {
        let mut p = qwen();
        let out = collect(&mut p, &["<tool_call>not json</tool_call>"]);
        assert!(out.tool_calls.is_empty());
        assert_eq!(out.errors.len(), 1);
        assert!(matches!(
            out.errors[0].kind,
            crate::normalize::error::NormalizationErrorKind::MalformedToolCallJson { .. }
        ));
    }

    #[test]
    fn missing_name_field_is_malformed() {
        let mut p = qwen();
        let out = collect(&mut p, &[r#"<tool_call>{"arguments":{}}</tool_call>"#]);
        assert!(out.tool_calls.is_empty());
        assert_eq!(out.errors.len(), 1);
    }

    #[test]
    fn missing_arguments_defaults_to_empty_object() {
        let mut p = qwen();
        let out = collect(&mut p, &[r#"<tool_call>{"name":"foo"}</tool_call>"#]);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].arguments, json!({}));
        assert!(out.errors.is_empty());
    }

    #[test]
    fn unclosed_tag_at_end_yields_error() {
        let mut p = qwen();
        let _ = p.push_text(r#"hello <tool_call>{"name":"foo""#);
        let f = p.finish();
        assert_eq!(f.errors.len(), 1);
        assert!(matches!(
            f.errors[0].kind,
            crate::normalize::error::NormalizationErrorKind::UnclosedToolCallTag { .. }
        ));
        assert!(f.tool_calls.is_empty());
    }

    #[test]
    fn multiple_tool_calls_get_distinct_ids() {
        let mut p = qwen();
        let out = collect(
            &mut p,
            &[
                r#"<tool_call>{"name":"a","arguments":{}}</tool_call>"#,
                r#"<tool_call>{"name":"b","arguments":{}}</tool_call>"#,
            ],
        );
        assert_eq!(out.tool_calls.len(), 2);
        assert_eq!(out.tool_calls[0].id, "call_qwen_0");
        assert_eq!(out.tool_calls[1].id, "call_qwen_1");
    }

    #[test]
    fn partial_marker_lookalike_is_eventually_flushed() {
        // "<tool" looks like an open-marker prefix but is actually just
        // text — finish() should flush it.
        let mut p = qwen();
        let mid = p.push_text("<tool");
        assert_eq!(mid.forward_text, "");
        let f = p.finish();
        assert_eq!(f.forward_text, "<tool");
    }

    #[test]
    fn partial_suffix_len_finds_longest_overlap() {
        assert_eq!(partial_suffix_len(b"abc<tool", b"<tool_call>"), 5);
        assert_eq!(partial_suffix_len(b"abc<", b"<tool_call>"), 1);
        assert_eq!(partial_suffix_len(b"abc", b"<tool_call>"), 0);
        // A full-marker suffix is *not* a partial — only proper prefixes
        // (1..len) count.  A full match is `find`'s job upstream.
        assert_eq!(partial_suffix_len(b"<tool_call>", b"<tool_call>"), 0);
        // The longest proper prefix that the buffer ends with is "<".
        assert_eq!(partial_suffix_len(b"</tool_call><", b"<tool_call>"), 1);
    }

    // -------------------------------------------------------------------
    // Inner-XML (`<function=…><parameter=…>…</parameter></function>`) —
    // the Qwen3 + `--jinja` tool-call body shape.
    // -------------------------------------------------------------------

    #[test]
    fn extracts_function_xml_body_with_string_param() {
        let mut p = qwen();
        let body = "<tool_call>\n<function=grep>\n<parameter=regex>\ngglib\\s+q\n</parameter>\n</function>\n</tool_call>";
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "grep");
        assert_eq!(
            out.tool_calls[0].arguments,
            json!({ "regex": "gglib\\s+q" })
        );
    }

    #[test]
    fn function_xml_body_with_multiple_params() {
        let mut p = qwen();
        let body = concat!(
            "<tool_call><function=read_file>",
            "<parameter=path>src/main.rs</parameter>",
            "<parameter=start_line>1</parameter>",
            "<parameter=end_line>20</parameter>",
            "</function></tool_call>",
        );
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty());
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "read_file");
        assert_eq!(
            out.tool_calls[0].arguments,
            json!({ "path": "src/main.rs", "start_line": 1, "end_line": 20 })
        );
    }

    #[test]
    fn function_xml_body_with_json_object_param() {
        let mut p = qwen();
        let body = r#"<tool_call><function=run><parameter=opts>{"a":1,"b":[2,3]}</parameter></function></tool_call>"#;
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty());
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(
            out.tool_calls[0].arguments,
            json!({ "opts": { "a": 1, "b": [2, 3] } })
        );
    }

    #[test]
    fn function_xml_body_streamed_byte_by_byte() {
        let mut p = qwen();
        let s = "<tool_call><function=grep><parameter=regex>x</parameter></function></tool_call>";
        let chunks: Vec<String> = s.chars().map(|c| c.to_string()).collect();
        let refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        let out = collect(&mut p, &refs);
        assert!(out.errors.is_empty());
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "grep");
        assert_eq!(out.tool_calls[0].arguments, json!({ "regex": "x" }));
    }

    #[test]
    fn function_xml_body_without_parameters_yields_empty_args() {
        let mut p = qwen();
        let body = "<tool_call><function=ping></function></tool_call>";
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty());
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].name, "ping");
        assert_eq!(out.tool_calls[0].arguments, json!({}));
    }

    /// Multiple `<function=...>` blocks back-to-back inside one `<tool_call>`
    /// wrapper (Hermes-style multi-call) must all be extracted, in order,
    /// with distinct synthesised IDs.
    #[test]
    fn multiple_function_blocks_in_one_wrapper_are_all_extracted() {
        let mut p = qwen();
        let body = concat!(
            "<tool_call>",
            "<function=get_weather><parameter=city>Paris</parameter></function>",
            "<function=get_time><parameter=zone>UTC</parameter></function>",
            "</tool_call>",
        );
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
        assert_eq!(out.tool_calls.len(), 2);
        assert_eq!(out.tool_calls[0].name, "get_weather");
        assert_eq!(out.tool_calls[0].arguments, json!({"city": "Paris"}));
        assert_eq!(out.tool_calls[1].name, "get_time");
        assert_eq!(out.tool_calls[1].arguments, json!({"zone": "UTC"}));
        assert_ne!(
            out.tool_calls[0].id, out.tool_calls[1].id,
            "each call in the block needs its own ID"
        );
    }

    /// A value that happens to contain the literal text `</parameter>` must
    /// not truncate the value early — the true close is the last occurrence
    /// before the next sibling tag, not the first occurrence anywhere. This
    /// is the naive-`find` bug: the old implementation would have stopped at
    /// "Use ", left `to close a param` dangling as unparsed cursor bytes, and
    /// failed the whole block.
    #[test]
    fn a_parameter_value_containing_the_literal_close_marker_does_not_truncate() {
        let mut p = qwen();
        let body = concat!(
            "<tool_call><function=write_doc>",
            "<parameter=text>Use </parameter> to close a param</parameter>",
            "<parameter=lang>en</parameter>",
            "</function></tool_call>",
        );
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(
            out.tool_calls[0].arguments,
            json!({"text": "Use </parameter> to close a param", "lang": "en"})
        );
    }

    /// Same rule at the `</function>` boundary: a parameter value containing
    /// the literal text `</function>` must not truncate the function body
    /// early when another sibling function follows.
    #[test]
    fn a_parameter_value_containing_the_literal_function_close_does_not_truncate() {
        let mut p = qwen();
        let body = concat!(
            "<tool_call>",
            "<function=write_doc><parameter=text>end with </function> tag</parameter></function>",
            "<function=ping></function>",
            "</tool_call>",
        );
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
        assert_eq!(out.tool_calls.len(), 2);
        assert_eq!(
            out.tool_calls[0].arguments,
            json!({"text": "end with </function> tag"})
        );
        assert_eq!(out.tool_calls[1].name, "ping");
    }

    /// A body that opens the XML dialect (`<function=`) but is structurally
    /// broken must be reported with the XML-specific error kind, not the
    /// generic JSON one — the two dialects fail for unrelated reasons.
    #[test]
    fn malformed_function_xml_gets_its_own_error_kind() {
        let mut p = qwen();
        let out = collect(
            &mut p,
            &["<tool_call><function=oops(no closing angle</tool_call>"],
        );
        assert!(out.tool_calls.is_empty());
        assert_eq!(out.errors.len(), 1);
        assert!(matches!(
            out.errors[0].kind,
            crate::normalize::error::NormalizationErrorKind::MalformedFunctionXml { .. }
        ));
    }

    /// Pinning the type-coercion limitation documented on
    /// `parse_param_value`: a parameter meant to be the literal string
    /// `"true"` or `"123"` is indistinguishable from one meant to be the
    /// boolean or the number, since the dialect carries no schema. This is
    /// not a bug to fix here — the parser has no `input_schema` to consult —
    /// but the behaviour must not change silently.
    #[test]
    fn parameter_values_that_look_like_json_literals_are_coerced_not_kept_as_strings() {
        let mut p = qwen();
        let body = concat!(
            "<tool_call><function=configure>",
            "<parameter=enabled>true</parameter>",
            "<parameter=count>123</parameter>",
            "<parameter=label>plain text</parameter>",
            "</function></tool_call>",
        );
        let out = collect(&mut p, &[body]);
        assert!(out.errors.is_empty());
        assert_eq!(
            out.tool_calls[0].arguments,
            json!({"enabled": true, "count": 123, "label": "plain text"}),
            "bool- and number-shaped strings coerce; only non-JSON-shaped text stays a string"
        );
    }

    // -------------------------------------------------------------------
    // Spec-driven behaviour: the same machine must work for any markers
    // and honor the spec's codec list and ID prefix.
    // -------------------------------------------------------------------

    /// A synthetic template-derived spec: custom multibyte markers, JSON
    /// body only, `call_dialect_` prefix.
    fn derived() -> DialectSpec {
        DialectSpec {
            id: crate::domain::dialect::DERIVED_DIALECT_ID.to_owned(),
            tool_open: "«TC»".to_owned(),
            tool_close: "«/TC»".to_owned(),
            body_codecs: vec![BodyCodec::Json],
            emission: crate::domain::dialect::EmissionProfile::default(),
            id_prefix: crate::domain::dialect::DERIVED_ID_PREFIX.to_owned(),
        }
    }

    #[test]
    fn custom_marker_spec_extracts_calls_with_its_own_id_prefix() {
        let mut p = DelimitedToolCallParser::new(derived());
        let out = collect(
            &mut p,
            &[r#"before«TC»{"name":"foo","arguments":{"x":1}}«/TC»after"#],
        );
        assert_eq!(out.forward_text, "beforeafter");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].id, "call_dialect_0");
        assert_eq!(out.tool_calls[0].name, "foo");
        assert!(out.errors.is_empty());
    }

    #[test]
    fn custom_marker_spec_survives_byte_at_a_time_chunking() {
        let mut p = DelimitedToolCallParser::new(derived());
        let s = r#"x«TC»{"name":"f","arguments":{"a":2}}«/TC»y"#;
        // Char-by-char, not byte-by-byte: push_text takes &str, and the
        // guillemet markers are multibyte.  Chunks of one char still split
        // every marker across pushes, which is the property under test.
        let chunks: Vec<String> = s.chars().map(|c| c.to_string()).collect();
        let refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        let out = collect(&mut p, &refs);
        assert_eq!(out.forward_text, "xy");
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].arguments, json!({"a": 2}));
    }

    #[test]
    fn fenced_spec_with_identical_open_and_close_markers_works() {
        let spec = DialectSpec {
            tool_open: "@@TOOL@@".to_owned(),
            tool_close: "@@TOOL@@".to_owned(),
            ..derived()
        };
        let mut p = DelimitedToolCallParser::new(spec);
        let out = collect(
            &mut p,
            &[r#"a@@TOOL@@{"name":"f","arguments":{}}@@TOOL@@b"#],
        );
        assert_eq!(out.forward_text, "ab");
        assert_eq!(out.tool_calls.len(), 1);
        assert!(out.errors.is_empty());
    }

    /// A JSON-only spec must not decode inner-XML bodies, and its failure
    /// must report the JSON error kind — the XML kind belongs to specs
    /// that actually enable the codec.
    #[test]
    fn json_only_spec_rejects_function_xml_with_the_json_error_kind() {
        let spec = DialectSpec {
            tool_open: "<tool_call>".to_owned(),
            tool_close: "</tool_call>".to_owned(),
            ..derived()
        };
        let mut p = DelimitedToolCallParser::new(spec);
        let out = collect(
            &mut p,
            &["<tool_call><function=grep><parameter=regex>x</parameter></function></tool_call>"],
        );
        assert!(out.tool_calls.is_empty());
        assert_eq!(out.errors.len(), 1);
        assert!(matches!(
            out.errors[0].kind,
            crate::normalize::error::NormalizationErrorKind::MalformedToolCallJson { .. }
        ));
    }

    /// The spec ↔ parser round trip: whatever `render_call` emits, a parser
    /// built from the same spec must extract — for the builtin and for a
    /// derived spec alike.  This is the property the grammar test in
    /// `request_pipeline::constrain` builds on.
    #[test]
    fn render_call_output_round_trips_through_a_parser_of_the_same_spec() {
        for spec in [DialectSpec::qwen_xml(), derived()] {
            let emission = spec.render_call("read_file", &json!({"path": "a.rs"}));
            let mut p = DelimitedToolCallParser::new(spec);
            let out = collect(&mut p, &[emission.as_str()]);
            assert!(out.errors.is_empty(), "errors: {:?}", out.errors);
            assert_eq!(out.tool_calls.len(), 1);
            assert_eq!(out.tool_calls[0].name, "read_file");
            assert_eq!(out.tool_calls[0].arguments, json!({"path": "a.rs"}));
            assert_eq!(out.forward_text, "");
        }
    }
}
