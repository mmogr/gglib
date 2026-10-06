//! What each event is on the wire: the frame an `OpenAI` client reads.

use super::*;
use crate::normalize::NormalizationErrorKind;

pub(super) fn enc() -> SseEncoder {
    SseEncoder::new("chatcmpl-1", "test-model", 1_729_000_000)
}

pub(super) fn parse_data_frame(out: &str) -> serde_json::Value {
    let line = out.lines().next().expect("non-empty output");
    let payload = line.strip_prefix("data: ").expect("data: prefix");
    serde_json::from_str(payload).expect("valid JSON")
}

#[test]
fn text_delta_encodes_to_content_chunk() {
    let out = enc()
        .encode(&LlmStreamEvent::TextDelta {
            content: "hello".to_owned(),
        })
        .expect("frame");
    assert!(out.starts_with("data: "));
    assert!(out.ends_with("\n\n"));
    let v = parse_data_frame(&out);
    assert_eq!(v["object"], "chat.completion.chunk");
    assert_eq!(v["id"], "chatcmpl-1");
    assert_eq!(v["model"], "test-model");
    assert_eq!(v["choices"][0]["delta"]["content"], "hello");
    assert!(v["choices"][0]["finish_reason"].is_null());
}

#[test]
fn reasoning_delta_encodes_to_reasoning_content_chunk() {
    let out = enc()
        .encode(&LlmStreamEvent::ReasoningDelta {
            content: "think".to_owned(),
        })
        .expect("frame");
    let v = parse_data_frame(&out);
    assert_eq!(v["choices"][0]["delta"]["reasoning_content"], "think");
}

#[test]
fn tool_call_delta_first_frame_includes_id_and_type() {
    let out = enc()
        .encode(&LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some("tc1".to_owned()),
            name: Some("search".to_owned()),
            arguments: Some(r#"{"q":"r"}"#.to_owned()),
        })
        .expect("frame");
    let v = parse_data_frame(&out);
    let tc = &v["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(tc["index"], 0);
    assert_eq!(tc["id"], "tc1");
    assert_eq!(tc["type"], "function");
    assert_eq!(tc["function"]["name"], "search");
    assert_eq!(tc["function"]["arguments"], r#"{"q":"r"}"#);
}

#[test]
fn tool_call_delta_continuation_omits_id_and_type() {
    let out = enc()
        .encode(&LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments: Some("more".to_owned()),
        })
        .expect("frame");
    let v = parse_data_frame(&out);
    let tc = &v["choices"][0]["delta"]["tool_calls"][0];
    assert!(tc.get("id").is_none(), "id must be omitted on continuation");
    assert!(
        tc.get("type").is_none(),
        "type must be omitted on continuation"
    );
    assert_eq!(tc["function"]["arguments"], "more");
}

#[test]
fn done_event_emits_only_finish_chunk_no_sentinel() {
    let out = enc()
        .encode(&LlmStreamEvent::Done {
            finish_reason: Some("stop".to_owned()),
        })
        .expect("frame");
    // Exactly one SSE frame -- [DONE] is the caller's responsibility now
    // (see DONE_SENTINEL doc), since a trailing Usage event can
    // legitimately follow Done.
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "Done emits exactly one data: line");
    let v: serde_json::Value =
        serde_json::from_str(lines[0].strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(v["choices"][0]["finish_reason"], "stop");
}

#[test]
fn usage_event_encodes_to_trailing_chunk_with_empty_choices() {
    let out = enc()
        .encode(&LlmStreamEvent::Usage {
            prompt_tokens: 123,
            completion_tokens: 45,
            total_tokens: 168,
            cached_tokens: None,
        })
        .expect("frame");
    let v = parse_data_frame(&out);
    assert_eq!(v["object"], "chat.completion.chunk");
    assert_eq!(v["id"], "chatcmpl-1");
    assert_eq!(v["model"], "test-model");
    assert!(
        v["choices"].as_array().is_some_and(Vec::is_empty),
        "usage chunk must carry an empty choices array, not omit it"
    );
    assert_eq!(v["usage"]["prompt_tokens"], 123);
    assert_eq!(v["usage"]["completion_tokens"], 45);
    assert_eq!(v["usage"]["total_tokens"], 168);
    assert!(
        v["usage"].get("prompt_tokens_details").is_none(),
        "an unreported cached-token count must not synthesize the details object"
    );
}

/// A reported count is re-emitted under the OpenAI-standard nesting, so
/// clients (e.g. the Copilot LLM Gateway extension's `promptTokenDetails`)
/// see it exactly where they expect.
#[test]
fn usage_event_re_emits_a_reported_cached_token_count() {
    let out = enc()
        .encode(&LlmStreamEvent::Usage {
            prompt_tokens: 123,
            completion_tokens: 45,
            total_tokens: 168,
            cached_tokens: Some(100),
        })
        .expect("frame");
    let v = parse_data_frame(&out);
    assert_eq!(v["usage"]["prompt_tokens_details"]["cached_tokens"], 100);
}

/// Zero reused tokens is a real measurement, not a missing one, so it must
/// survive encoding rather than being elided like `None`.
#[test]
fn usage_event_distinguishes_zero_cached_tokens_from_absent() {
    let out = enc()
        .encode(&LlmStreamEvent::Usage {
            prompt_tokens: 123,
            completion_tokens: 45,
            total_tokens: 168,
            cached_tokens: Some(0),
        })
        .expect("frame");
    let v = parse_data_frame(&out);
    assert_eq!(v["usage"]["prompt_tokens_details"]["cached_tokens"], 0);
}

#[test]
fn upstream_error_event_encodes_to_bare_error_frame_no_sentinel() {
    let out = enc()
        .encode(&LlmStreamEvent::UpstreamError {
            message: "Context window limit reached.".to_owned(),
            error_type: "context_length_exceeded".to_owned(),
            code: "context_length_exceeded".to_owned(),
        })
        .expect("frame");
    let lines: Vec<&str> = out.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 1, "expects only the bare error frame");
    let v: serde_json::Value =
        serde_json::from_str(lines[0].strip_prefix("data: ").unwrap()).unwrap();
    assert_eq!(v["error"]["message"], "Context window limit reached.");
    assert_eq!(v["error"]["type"], "context_length_exceeded");
    assert_eq!(v["error"]["code"], "context_length_exceeded");
    assert!(
        v.get("choices").is_none(),
        "inline error frame must not carry a choices key at all"
    );
    assert!(
        v.get("id").is_none(),
        "inline error frame is deliberately bare, no envelope fields"
    );
}

/// The frame byte for byte, since this is the one place it is written and
/// clients parse it: ggchat's `WireTests` quote this envelope, keys in this
/// order.
#[test]
fn the_error_frame_is_these_bytes() {
    assert_eq!(
        SseEncoder::upstream_error_frame("gone", "server_error", "upstream_error"),
        concat!(
            r#"data: {"error":{"code":"upstream_error","message":"gone","type":"server_error"}}"#,
            "\n\n",
        )
    );
}

#[test]
fn prompt_progress_encodes_to_top_level_field() {
    let out = enc()
        .encode(&LlmStreamEvent::PromptProgress {
            processed: 2,
            total: 8,
            cached: 1,
            time_ms: 100,
        })
        .expect("frame");
    let v = parse_data_frame(&out);
    assert_eq!(v["prompt_progress"]["processed"], 2);
    assert_eq!(v["prompt_progress"]["total"], 8);
    assert_eq!(v["prompt_progress"]["cache"], 1);
    assert_eq!(v["prompt_progress"]["time_ms"], 100);
    assert!(v.get("choices").is_none());
}

#[test]
fn normalization_error_is_suppressed() {
    let out = enc().encode(&LlmStreamEvent::NormalizationError {
        kind: NormalizationErrorKind::MalformedToolCallJson {
            raw: "<tool_call>oops".to_owned(),
        },
        raw: "<tool_call>oops".to_owned(),
    });
    assert!(
        out.is_none(),
        "NormalizationError must never reach the wire"
    );
}
