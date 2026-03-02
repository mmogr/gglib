//! Parser for OpenAI-compatible SSE `data:` frames.
//!
//! Isolated from [`super::llm_completion`] so the frame-parsing logic and its
//! tests are self-contained and do not require an HTTP client or async runtime.
//!
//! # Frame ordering for reasoning models
//!
//! When a single SSE frame carries both `reasoning_content` (chain-of-thought)
//! **and** `content` (answer text), the [`ReasoningDelta`] event is emitted
//! first.  This matches the temporal semantics of reasoning models such as
//! DeepSeek R1 and QwQ, where the chain-of-thought is always produced before
//! the answer — even if llama-server coalesces both into the same frame.
//!
//! [`ReasoningDelta`]: gglib_core::LlmStreamEvent::ReasoningDelta

use anyhow::{Result, anyhow};
use gglib_core::domain::agent::LlmStreamEvent;

// =============================================================================
// Public(crate) types
// =============================================================================

/// Result of parsing a single SSE `data:` payload.
#[derive(Debug)]
pub(crate) enum SseParseResult {
    /// The value `[DONE]` — stream terminator, no events.
    Done,
    /// One or more events decoded from the JSON frame.
    Events(Vec<LlmStreamEvent>),
}

// =============================================================================
// Parser
// =============================================================================

/// Parse a single SSE `data:` payload into zero or more [`LlmStreamEvent`]s.
///
/// Returns:
/// - `Ok(SseParseResult::Done)` when `data == "[DONE]"`
/// - `Ok(SseParseResult::Events(…))` for a valid JSON frame (may be empty
///   when the frame carries no content or tool-call deltas)
/// - `Err(…)` when the frame is not valid JSON
pub(crate) fn parse_sse_frame(data: &str) -> Result<SseParseResult> {
    if data == "[DONE]" {
        return Ok(SseParseResult::Done);
    }

    let parsed: serde_json::Value = serde_json::from_str(data)
        .map_err(|e| anyhow!("SSE frame JSON parse error: {e} — data: {data}"))?;

    // Guard against keepalive / error frames that carry no `choices` array.
    // Without this check every field access falls through to `Value::Null`,
    // events are silently dropped, and a `finish_reason: "stop"` in such a
    // frame would mean the stream never emits `Done`.
    let choices = &parsed["choices"];
    if choices.as_array().map_or(true, |a| a.is_empty()) {
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
                .map(|i| i as usize)
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
        // A server that omits `index` on a single tool-call element should
        // assign it position 0 (first element), not silently collapse onto 0
        // from `unwrap_or(0)` which is the same value — but for TWO elements
        // both collapsing to 0 would data-lose the second call.
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
        // This is the critical regression test for the silent slot-collision bug:
        // two tool-call elements without `index` must be assigned slots 0 and 1,
        // not both slot 0 (which would cause the second to overwrite the first
        // in the stream collector's `partials` Vec).
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
        // When a single frame carries both reasoning_content (CoT) and content
        // (answer text), ReasoningDelta must appear before TextDelta because
        // chain-of-thought semantically precedes the answer.
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
}
