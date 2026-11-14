//! What each event is on the wire: the frame an `OpenAI` client reads.

use super::*;

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

/// gglib writes no frame of its own for reasoning, progress, usage or an
/// upstream error: those reach a client as llama-server wrote them.
#[test]
fn only_text_tool_call_and_finish_events_are_encoded() {
    for event in [
        LlmStreamEvent::ReasoningDelta {
            content: "think".to_owned(),
        },
        LlmStreamEvent::PromptProgress {
            processed: 1,
            total: 2,
            cached: 0,
            time_ms: 3,
        },
        LlmStreamEvent::Usage {
            prompt_tokens: 1,
            completion_tokens: 2,
            total_tokens: 3,
            cached_tokens: None,
        },
        LlmStreamEvent::UpstreamError {
            message: "gone".to_owned(),
            error_type: "server_error".to_owned(),
            code: "upstream_error".to_owned(),
        },
    ] {
        assert_eq!(enc().encode(&event), None, "{event:?}");
    }
}
