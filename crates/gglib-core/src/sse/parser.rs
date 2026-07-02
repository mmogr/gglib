//! Parser for OpenAI-compatible SSE `data:` frames.
//!
//! Isolated so the frame-parsing logic and its tests are self-contained and
//! do not require an HTTP client or async runtime.
//!
//! # Frame ordering for reasoning models
//!
//! When a single SSE frame carries both `reasoning_content` (chain-of-thought)
//! **and** `content` (answer text), the [`ReasoningDelta`] event is emitted
//! first.  This matches the temporal semantics of reasoning models such as
//! `DeepSeek` R1 and `QwQ`, where the chain-of-thought is always produced before
//! the answer — even if llama-server coalesces both into the same frame.
//!
//! [`ReasoningDelta`]: crate::LlmStreamEvent::ReasoningDelta

use anyhow::{Result, anyhow};

use crate::domain::agent::LlmStreamEvent;

// =============================================================================
// Public types
// =============================================================================

/// Result of parsing a single SSE `data:` payload.
#[derive(Debug)]
pub enum SseParseResult {
    /// The value `[DONE]` — stream terminator, no events.
    Done,
    /// One or more events decoded from the JSON frame.
    Events(Vec<LlmStreamEvent>),
}

// =============================================================================
// Parser
// =============================================================================

/// Parse a top-level `usage` object into a [`LlmStreamEvent::Usage`] event.
///
/// Returns `None` when the frame carries no `usage` field. Deliberately
/// returns just the event (not a full [`SseParseResult`]) — unlike
/// [`parse_inline_error_frame`], a `usage` field does **not** imply the rest
/// of the frame should be skipped. See the call site in [`parse_sse_frame`]
/// for why.
fn parse_usage_event(parsed: &serde_json::Value) -> Option<LlmStreamEvent> {
    let usage = parsed.get("usage")?;
    let prompt_tokens =
        u32::try_from(usage["prompt_tokens"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
    let completion_tokens =
        u32::try_from(usage["completion_tokens"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
    let total_tokens =
        u32::try_from(usage["total_tokens"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
    Some(LlmStreamEvent::Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens,
    })
}

/// Parse a bare top-level `error` object (with no `choices` key) into an
/// [`LlmStreamEvent::UpstreamError`] event.
///
/// Returns `None` when the frame carries no `error` field, or when it also
/// carries a `choices` key (even an empty one) — that shape doesn't match
/// what downstream clients detect as an inline error, so it falls through
/// to the remaining frame-shape checks instead.
fn parse_inline_error_frame(parsed: &serde_json::Value) -> Option<SseParseResult> {
    let err = parsed.get("error")?;
    if parsed.get("choices").is_some() {
        return None;
    }
    let (message, error_type, code) = match err {
        serde_json::Value::String(s) => (
            s.clone(),
            "server_error".to_owned(),
            "upstream_error".to_owned(),
        ),
        _ => (
            err.get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("upstream returned an error")
                .to_owned(),
            err.get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("server_error")
                .to_owned(),
            err.get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("upstream_error")
                .to_owned(),
        ),
    };
    Some(SseParseResult::Events(vec![
        LlmStreamEvent::UpstreamError {
            message,
            error_type,
            code,
        },
    ]))
}

/// Parse a single SSE `data:` payload into zero or more [`LlmStreamEvent`]s.
///
/// Returns:
/// - `Ok(SseParseResult::Done)` when `data == "[DONE]"`
/// - `Ok(SseParseResult::Events(…))` for a valid JSON frame (may be empty
///   when the frame carries no content or tool-call deltas)
/// - `Err(…)` when the frame is not valid JSON
///
/// # Errors
///
/// Returns an error if the `data` payload is not valid JSON.
pub fn parse_sse_frame(data: &str) -> Result<SseParseResult> {
    if data == "[DONE]" {
        return Ok(SseParseResult::Done);
    }

    let parsed: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| anyhow!("SSE frame JSON parse error: {e} — data: {data}"))?;

    // ── Prompt-progress frames (llama-server `return_progress: true`) ────
    // These arrive during the pre-fill phase and have no `choices` array.
    // We check for them *before* the choices guard so they aren't silently
    // dropped as "no choices" frames.
    if let Some(pp) = parsed.get("prompt_progress") {
        let processed = u32::try_from(pp["processed"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
        let total = u32::try_from(pp["total"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
        let cached = u32::try_from(pp["cache"].as_u64().unwrap_or(0)).unwrap_or(u32::MAX);
        let time_ms = pp["time_ms"].as_u64().unwrap_or(0);
        return Ok(SseParseResult::Events(vec![
            LlmStreamEvent::PromptProgress {
                processed,
                total,
                cached,
                time_ms,
            },
        ]));
    }

    // ── Usage totals (`stream_options.include_usage: true`) ──────────────
    // Extracted here but *not* an early return: strict OpenAI servers send
    // this on a trailing chunk with empty `choices`, but llama-server
    // attaches `usage` directly onto the same chunk that also carries a
    // real `finish_reason` and non-empty `choices`
    // (ggml-org/llama.cpp#12102, #15443). Treating `usage` presence as a
    // reason to skip the rest of the frame would silently discard that
    // finish_reason/delta. Decided below once we know whether `choices`
    // is actually empty.
    let usage_event = parse_usage_event(&parsed);

    // ── Inline upstream error frame ───────────────────────────────────────
    // Some OpenAI-compatible servers (including llama.cpp) can emit a bare
    // `{"error": {...}}` frame mid-stream instead of a hard HTTP-level
    // failure (e.g. a context-length overflow discovered only once
    // generation is underway). Checked *before* the choices guard below for
    // the same reason as `prompt_progress`/`usage`: this frame has no
    // `choices` key at all, and clients such as the GitHub Copilot LLM
    // Gateway extension specifically detect this shape via
    // `'error' in obj && !('choices' in obj)` to surface a real error
    // instead of hanging or seeing a silently truncated response.
    if let Some(result) = parse_inline_error_frame(&parsed) {
        return Ok(result);
    }

    // Guard against keepalive / error frames that carry no `choices` array.
    // Without this check every field access falls through to `Value::Null`,
    // events are silently dropped, and a `finish_reason: "stop"` in such a
    // frame would mean the stream never emits `Done`.
    let choices = &parsed["choices"];
    if choices.as_array().is_none_or(Vec::is_empty) {
        // Strict-OpenAI shape: no real choice, usage (if any) stands alone.
        if let Some(usage_event) = usage_event {
            return Ok(SseParseResult::Events(vec![usage_event]));
        }
        tracing::debug!(data = %data, "SSE frame has no 'choices' entries — skipping");
        return Ok(SseParseResult::Events(vec![]));
    }
    let choice = &choices[0];
    let delta = &choice["delta"];

    let mut events: Vec<LlmStreamEvent> = Vec::new();

    // ── Reasoning/CoT content delta (DeepSeek R1 / QwQ) ────────────────────
    // Emitted FIRST: chain-of-thought semantically precedes answer text, so
    // even when both fields appear in the same frame we preserve this order.
    // llama-server emits `delta["reasoning_content"]` when started with
    // `--reasoning-format deepseek`.
    if let Some(reasoning) = delta["reasoning_content"].as_str()
        && !reasoning.is_empty()
    {
        events.push(LlmStreamEvent::ReasoningDelta {
            content: reasoning.to_owned(),
        });
    }

    // ── Text content delta ──────────────────────────────────────────────────
    if let Some(content) = delta["content"].as_str()
        && !content.is_empty()
    {
        events.push(LlmStreamEvent::TextDelta {
            content: content.to_owned(),
        });
    }

    // ── Tool-call deltas ────────────────────────────────────────────────────
    if let Some(tool_calls) = delta["tool_calls"].as_array() {
        for (sequential, tc) in tool_calls.iter().enumerate() {
            // Prefer the explicit `index` field; fall back to the element's
            // position in the array when `index` is absent.  A server that
            // omits `index` on every element is non-compliant with the OpenAI
            // spec, but we handle it gracefully rather than silently collapsing
            // all calls onto slot 0.
            let index = tc["index"]
                .as_u64()
                .and_then(|i| usize::try_from(i).ok())
                .unwrap_or(sequential);
            let id = tc["id"].as_str().map(str::to_owned);
            let name = tc["function"]["name"].as_str().map(str::to_owned);
            let arguments = tc["function"]["arguments"].as_str().map(str::to_owned);
            events.push(LlmStreamEvent::ToolCallDelta {
                index,
                id,
                name,
                arguments,
            });
        }
    }

    // ── Usage totals bundled with this finish chunk (llama.cpp shape) ────
    // Pushed *before* the finish_reason/Done event below, never after: the
    // encoder appends the `[DONE]` sentinel immediately after `Done`, and
    // nothing may follow `[DONE]` on the wire.
    if let Some(usage_event) = usage_event {
        events.push(usage_event);
    }

    // ── Finish reason → Done ────────────────────────────────────────────────
    if let Some(finish_reason) = choice["finish_reason"].as_str()
        && !finish_reason.is_empty()
    {
        events.push(LlmStreamEvent::Done {
            finish_reason: finish_reason.to_owned(),
        });
    }

    Ok(SseParseResult::Events(events))
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ── Frame builders ─────────────────────────────────────────────────────

    fn text_frame(content: &str) -> String {
        serde_json::json!({
            "choices": [{ "delta": { "content": content }, "finish_reason": null }]
        })
        .to_string()
    }

    fn finish_frame(reason: &str) -> String {
        serde_json::json!({
            "choices": [{ "delta": {}, "finish_reason": reason }]
        })
        .to_string()
    }

    fn tool_frame(index: usize, id: &str, name: &str, args: &str) -> String {
        serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": index,
                        "id": id,
                        "function": { "name": name, "arguments": args }
                    }]
                },
                "finish_reason": null
            }]
        })
        .to_string()
    }

    /// Build a frame whose `tool_calls` array elements intentionally omit the
    /// `index` field, simulating a non-compliant but real-world server.
    fn tool_frame_no_index(id: &str, name: &str, args: &str) -> String {
        serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [
                        { "id": id, "function": { "name": name, "arguments": args } }
                    ]
                },
                "finish_reason": null
            }]
        })
        .to_string()
    }

    /// Build a frame with two tool-call elements that both omit `index`.
    fn two_tool_frames_no_index() -> String {
        serde_json::json!({
            "choices": [{
                "delta": {
                    "tool_calls": [
                        { "id": "c1", "function": { "name": "search",  "arguments": "{}" } },
                        { "id": "c2", "function": { "name": "read_file", "arguments": "{}" } }
                    ]
                },
                "finish_reason": null
            }]
        })
        .to_string()
    }

    // ── Tests ──────────────────────────────────────────────────────────────

    #[test]
    fn done_sentinel_returns_done_variant() {
        assert!(matches!(
            parse_sse_frame("[DONE]"),
            Ok(SseParseResult::Done)
        ));
    }

    #[test]
    fn text_delta_frame_produces_text_event() {
        let events = match parse_sse_frame(&text_frame("hello")) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::TextDelta { content } if content == "hello"
        ));
    }

    #[test]
    fn empty_content_produces_no_text_event() {
        let frame = serde_json::json!({
            "choices": [{ "delta": { "content": "" }, "finish_reason": null }]
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(
            events.is_empty(),
            "empty content should not produce TextDelta"
        );
    }

    #[test]
    fn finish_reason_produces_done_event() {
        let events = match parse_sse_frame(&finish_frame("stop")) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::Done { finish_reason } if finish_reason == "stop"
        ));
    }

    #[test]
    fn tool_call_delta_frame_is_parsed() {
        let events = match parse_sse_frame(&tool_frame(0, "tc1", "search", r#"{"q":"rust"}"#)) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::ToolCallDelta {
                index: 0,
                id: Some(id),
                name: Some(n),
                arguments: Some(a),
            } if id == "tc1" && n == "search" && a == r#"{"q":"rust"}"#
        ));
    }

    #[test]
    fn tool_call_delta_with_no_index_defaults_to_sequential_position() {
        let events = match parse_sse_frame(&tool_frame_no_index("tc1", "search", r#"{"q":"rust"}"#))
        {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::ToolCallDelta { index: 0, id: Some(id), .. } if id == "tc1"
        ));
    }

    #[test]
    fn two_tool_calls_with_no_index_get_distinct_sequential_slots() {
        let events = match parse_sse_frame(&two_tool_frames_no_index()) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 2, "both tool-call deltas must be emitted");
        assert!(matches!(
            &events[0],
            LlmStreamEvent::ToolCallDelta { index: 0, id: Some(id), .. } if id == "c1"
        ));
        assert!(matches!(
            &events[1],
            LlmStreamEvent::ToolCallDelta { index: 1, id: Some(id), .. } if id == "c2"
        ));
    }

    #[test]
    fn malformed_json_returns_error() {
        assert!(
            parse_sse_frame("{ broken json }").is_err(),
            "malformed JSON should return Err"
        );
    }

    #[test]
    fn frame_with_text_and_finish_reason_produces_both_events() {
        let frame = serde_json::json!({
            "choices": [{ "delta": { "content": "hi" }, "finish_reason": "stop" }]
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], LlmStreamEvent::TextDelta { .. }));
        assert!(matches!(&events[1], LlmStreamEvent::Done { .. }));
    }

    #[test]
    fn reasoning_content_produces_reasoning_delta_event() {
        let frame = serde_json::json!({
            "choices": [{ "delta": { "reasoning_content": "I should check..." }, "finish_reason": null }]
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::ReasoningDelta { content } if content == "I should check..."
        ));
    }

    #[test]
    fn empty_reasoning_content_produces_no_event() {
        let frame = serde_json::json!({
            "choices": [{ "delta": { "reasoning_content": "" }, "finish_reason": null }]
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(
            events.is_empty(),
            "empty reasoning_content should not produce ReasoningDelta"
        );
    }

    #[test]
    fn frame_with_reasoning_and_text_reasoning_emitted_first() {
        let frame = serde_json::json!({
            "choices": [{ "delta": { "content": "ok", "reasoning_content": "think" }, "finish_reason": null }]
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 2);
        assert!(
            matches!(&events[0], LlmStreamEvent::ReasoningDelta { content } if content == "think"),
            "ReasoningDelta must come first"
        );
        assert!(
            matches!(&events[1], LlmStreamEvent::TextDelta { content } if content == "ok"),
            "TextDelta must come second"
        );
    }

    #[test]
    fn prompt_progress_frame_produces_progress_event() {
        let frame = serde_json::json!({
            "prompt_progress": {
                "processed": 2048,
                "total": 8192,
                "cache": 512,
                "time_ms": 1234
            }
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::PromptProgress {
                processed: 2048,
                total: 8192,
                cached: 512,
                time_ms: 1234
            }
        ));
    }

    #[test]
    fn prompt_progress_frame_not_confused_with_choices() {
        let frame = serde_json::json!({
            "prompt_progress": {
                "processed": 100,
                "total": 100,
                "cache": 0,
                "time_ms": 50
            }
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(
            !events.is_empty(),
            "prompt_progress frame must not be skipped"
        );
    }

    #[test]
    fn usage_frame_emits_usage_event() {
        let frame = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion.chunk",
            "created": 0,
            "model": "test-model",
            "choices": [],
            "usage": {
                "prompt_tokens": 123,
                "completion_tokens": 45,
                "total_tokens": 168
            }
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::Usage {
                prompt_tokens: 123,
                completion_tokens: 45,
                total_tokens: 168
            }
        ));
    }

    #[test]
    fn usage_frame_not_confused_with_no_choices_guard() {
        // Empty `choices` array — would be silently dropped by the "no
        // choices" guard if the usage check didn't run first.
        let frame = serde_json::json!({
            "choices": [],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(!events.is_empty(), "usage frame must not be skipped");
    }

    #[test]
    fn llama_cpp_combined_usage_and_finish_chunk_emits_both_events() {
        // Real llama-server shape (ggml-org/llama.cpp#12102, #15443): usage
        // is attached to the *same* chunk as a real finish_reason and a
        // non-empty `choices` array, not a separate trailing chunk with
        // empty choices. Must not silently drop the finish_reason.
        let frame = serde_json::json!({
            "choices": [{ "finish_reason": "tool_calls", "index": 0, "delta": {} }],
            "created": 0,
            "id": "chatcmpl-1",
            "model": "test-model",
            "object": "chat.completion.chunk",
            "usage": { "prompt_tokens": 4181, "completion_tokens": 12, "total_tokens": 4193 }
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 2, "expected both a Usage and a Done event");
        // Usage must come *before* Done -- the encoder appends `[DONE]`
        // immediately after Done, so nothing may be emitted after it.
        assert!(matches!(
            &events[0],
            LlmStreamEvent::Usage {
                prompt_tokens: 4181,
                completion_tokens: 12,
                total_tokens: 4193
            }
        ));
        assert!(matches!(
            &events[1],
            LlmStreamEvent::Done { finish_reason } if finish_reason == "tool_calls"
        ));
    }

    #[test]
    fn llama_cpp_combined_usage_and_stop_chunk_preserves_finish_reason() {
        let frame = serde_json::json!({
            "choices": [{ "finish_reason": "stop", "index": 0, "delta": {} }],
            "usage": { "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15 }
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], LlmStreamEvent::Usage { .. }));
        assert!(matches!(
            &events[1],
            LlmStreamEvent::Done { finish_reason } if finish_reason == "stop"
        ));
    }

    #[test]
    fn inline_error_frame_object_form_extracts_all_fields() {
        let frame = serde_json::json!({
            "error": {
                "message": "Context window limit reached.",
                "type": "context_length_exceeded",
                "code": "context_length_exceeded"
            }
        })
        .to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::UpstreamError { message, error_type, code }
                if message == "Context window limit reached."
                    && error_type == "context_length_exceeded"
                    && code == "context_length_exceeded"
        ));
    }

    #[test]
    fn inline_error_frame_string_form_uses_defaults() {
        let frame = serde_json::json!({ "error": "boom" }).to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            LlmStreamEvent::UpstreamError { message, error_type, code }
                if message == "boom" && error_type == "server_error" && code == "upstream_error"
        ));
    }

    #[test]
    fn inline_error_frame_not_dropped_by_no_choices_guard() {
        // No `choices` key at all -- would be silently dropped by the "no
        // choices" guard if the error check didn't run first.
        let frame = serde_json::json!({ "error": { "message": "oops" } }).to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(!events.is_empty(), "inline error frame must not be skipped");
    }

    #[test]
    fn error_alongside_choices_key_is_not_treated_as_inline_error() {
        // `choices` key present (even empty) means this isn't the bare
        // inline-error shape the extension detects via `!('choices' in
        // obj)` -- falls through to the normal "no choices" skip instead.
        let frame =
            serde_json::json!({ "error": { "message": "oops" }, "choices": [] }).to_string();
        let events = match parse_sse_frame(&frame) {
            Ok(SseParseResult::Events(e)) => e,
            other => panic!("unexpected: {other:?}"),
        };
        assert!(
            events.is_empty(),
            "frame with a choices key should not be parsed as UpstreamError"
        );
    }
}
