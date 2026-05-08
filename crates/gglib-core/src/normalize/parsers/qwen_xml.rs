//! Qwen-style XML tool-call parser.
//!
//! Rewrites embedded `<tool_call>{json}</tool_call>` markup — emitted by
//! Qwen 2 / 2.5 / 3 family models in either the text or reasoning channel —
//! into proper [`ToolCall`] values.  Bytes outside of `<tool_call>` regions
//! are forwarded verbatim on the channel they arrived on.
//!
//! ## Chunk safety
//!
//! Both the open marker (`<tool_call>`, 11 bytes) and the close marker
//! (`</tool_call>`, 12 bytes) may straddle SSE chunk boundaries.  The parser
//! holds back at most `CLOSE_MARKER.len() - 1 = 11` bytes per channel as a
//! lookahead buffer.  The buffered bytes are flushed on the next push or at
//! [`ToolCallParser::finish`].
//!
//! ## Cross-channel handling
//!
//! In practice a Qwen tool call appears entirely on one channel — either
//! text (no reasoning split) or reasoning (when `--reasoning-format` is on).
//! Each channel therefore maintains its own independent parser state
//! ([`ChannelState`]) so that markup never crosses channels.  The synthesised
//! tool-call IDs share a single monotonic counter across both channels.

use serde_json::Value;

use super::super::error::NormalizationError;
use super::super::parser::{ParserOutput, ToolCallParser};
use crate::domain::agent::ToolCall;

/// Open marker for a Qwen tool call.
const OPEN: &str = "<tool_call>";
/// Close marker for a Qwen tool call.
const CLOSE: &str = "</tool_call>";

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

/// Parser for the Qwen XML tool-call dialect.  See module docs.
#[derive(Default, Debug)]
pub struct QwenXmlParser {
    text: ChannelState,
    reasoning: ChannelState,
    /// Monotonic counter for synthesised tool-call IDs.  Shared across
    /// both channels so IDs remain globally unique within a single stream.
    next_id: u32,
}

impl QwenXmlParser {
    /// Construct a fresh parser with empty per-channel buffers.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a stream-unique synthetic ID for an extracted tool call.
    fn mint_id(&mut self) -> String {
        let n = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        format!("call_qwen_{n}")
    }

    /// Drive the state machine for one channel.
    ///
    /// All scanning logic lives here; `push_text` and `push_reasoning` are
    /// thin dispatch wrappers that pick the right `ChannelState` and route
    /// flushed bytes to the right output field.
    fn scan(&mut self, channel: Channel, chunk: &str) -> ParserOutput {
        let mut out = ParserOutput::default();

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
                if let Some(p) = state.pending.find(CLOSE) {
                    state.body.push_str(&state.pending[..p]);
                    finalize_tool_call(&state.body, &mut out, || self.mint_id());
                    state.body.clear();
                    state.inside = false;
                    state.pending.drain(..p + CLOSE.len());
                    continue;
                }
                let keep = partial_suffix_len(state.pending.as_bytes(), CLOSE.as_bytes());
                let flush_to = state.pending.len() - keep;
                state.body.push_str(&state.pending[..flush_to]);
                state.pending.drain(..flush_to);
                break;
            }

            // Outside any tool_call.
            if let Some(p) = state.pending.find(OPEN) {
                forward(&mut out, channel, &state.pending[..p]);
                state.pending.drain(..p + OPEN.len());
                state.inside = true;
                continue;
            }
            let keep = partial_suffix_len(state.pending.as_bytes(), OPEN.as_bytes());
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
            out.errors.push(NormalizationError::unclosed_tool_call(partial));
        } else {
            // Any held-back bytes turned out to be ordinary text — flush.
            forward(&mut out, channel, &state.pending);
        }
        out
    }
}

impl ToolCallParser for QwenXmlParser {
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

/// Parse the accumulated JSON body and push the resulting [`ToolCall`] (or
/// a [`NormalizationError`]) onto `out`.
fn finalize_tool_call(body: &str, out: &mut ParserOutput, mut mint_id: impl FnMut() -> String) {
    let trimmed = body.trim();
    let Ok(parsed) = serde_json::from_str::<Value>(trimmed) else {
        out.errors
            .push(NormalizationError::malformed_tool_call(body.to_owned()));
        return;
    };
    let Some(obj) = parsed.as_object() else {
        out.errors
            .push(NormalizationError::malformed_tool_call(body.to_owned()));
        return;
    };
    let Some(name) = obj.get("name").and_then(Value::as_str).map(str::to_owned) else {
        out.errors
            .push(NormalizationError::malformed_tool_call(body.to_owned()));
        return;
    };
    let arguments = obj
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
    out.tool_calls.push(ToolCall {
        id: mint_id(),
        name,
        arguments,
    });
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

    fn collect(p: &mut QwenXmlParser, chunks: &[&str]) -> ParserOutput {
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
        let mut p = QwenXmlParser::new();
        let out = collect(&mut p, &["hello ", "world"]);
        assert_eq!(out.forward_text, "hello world");
        assert!(out.tool_calls.is_empty());
        assert!(out.errors.is_empty());
    }

    #[test]
    fn extracts_simple_tool_call_from_text() {
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
        let out = collect(&mut p, &[r#"<tool_call>{"arguments":{}}</tool_call>"#]);
        assert!(out.tool_calls.is_empty());
        assert_eq!(out.errors.len(), 1);
    }

    #[test]
    fn missing_arguments_defaults_to_empty_object() {
        let mut p = QwenXmlParser::new();
        let out = collect(&mut p, &[r#"<tool_call>{"name":"foo"}</tool_call>"#]);
        assert_eq!(out.tool_calls.len(), 1);
        assert_eq!(out.tool_calls[0].arguments, json!({}));
        assert!(out.errors.is_empty());
    }

    #[test]
    fn unclosed_tag_at_end_yields_error() {
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
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
        let mut p = QwenXmlParser::new();
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
}
