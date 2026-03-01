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

    let choice = &parsed["choices"][0];
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
        for tc in tool_calls {
            let index = tc["index"].as_u64().unwrap_or(0) as usize;
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
        assert!(events.is_empty(), "empty content should not produce TextDelta");
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
        let events =
            match parse_sse_frame(&tool_frame(0, "tc1", "search", r#"{"q":"rust"}"#)) {
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
