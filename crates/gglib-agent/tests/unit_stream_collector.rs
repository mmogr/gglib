//! Unit tests for [`collect_stream`] — the streaming LLM response collector.
//!
//! These tests were extracted from inline `#[cfg(test)]` tests inside
//! `src/stream_collector.rs` to keep the source file focused on production
//! logic and to follow the same external-test pattern used by
//! `unit_agent_loop.rs`.
//!
//! # Coverage
//!
//! | Test | Scenario |
//! |------|----------|
//! | [`text_delta_forwarded_and_accumulated`] | Text deltas forwarded live and joined |
//! | [`reasoning_delta_forwarded_and_accumulated_separately`] | Reasoning kept separate from content |
//! | [`tool_call_deltas_assembled_into_tool_calls`] | Fragmented tool-call JSON reassembled |
//! | [`multiple_tool_calls_assembled_by_index`] | Multiple tool calls indexed correctly |
//! | [`zero_event_stream_returns_distinct_error`] | Empty stream → distinct error message |
//! | [`truncated_stream_returns_distinct_error`] | Missing Done → truncation error |
//! | [`malformed_json_arguments_emit_error_and_fail`] | Bad JSON → `AgentEvent::Error` + `Err` |
//! | [`oversized_tool_call_index_returns_error`] | Index ≥ `MAX_TOOL_CALL_INDEX` rejected |

use std::pin::Pin;

use anyhow::Result;
use futures_util::stream;
use gglib_agent::{MAX_TOOL_CALL_INDEX, collect_stream};
use gglib_core::{AgentEvent, LlmStreamEvent};
use tokio::sync::mpsc;

fn make_stream(
    events: Vec<LlmStreamEvent>,
) -> Pin<Box<dyn futures_core::Stream<Item = Result<LlmStreamEvent>> + Send>> {
    Box::pin(stream::iter(events.into_iter().map(Ok)))
}

#[tokio::test]
async fn text_delta_forwarded_and_accumulated() {
    let (tx, mut rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::TextDelta {
            content: "Hello, ".into(),
        },
        LlmStreamEvent::TextDelta {
            content: "world!".into(),
        },
        LlmStreamEvent::Done {
            finish_reason: "stop".into(),
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    assert_eq!(response.content, "Hello, world!");
    assert_eq!(response.finish_reason, "stop");
    assert!(response.tool_calls.is_empty());

    // Both text deltas should have been forwarded to the channel.
    let evt1 = rx.recv().await.unwrap();
    let evt2 = rx.recv().await.unwrap();
    assert!(matches!(evt1, AgentEvent::TextDelta { content } if content == "Hello, "));
    assert!(matches!(evt2, AgentEvent::TextDelta { content } if content == "world!"));
}

#[tokio::test]
async fn reasoning_delta_forwarded_and_accumulated_separately() {
    let (tx, mut rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::ReasoningDelta {
            content: "Let me think".into(),
        },
        LlmStreamEvent::ReasoningDelta {
            content: " about this.".into(),
        },
        LlmStreamEvent::TextDelta {
            content: "Answer.".into(),
        },
        LlmStreamEvent::Done {
            finish_reason: "stop".into(),
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    // Reasoning content is accumulated separately and NOT mixed into content.
    assert_eq!(response.reasoning_content, "Let me think about this.");
    assert_eq!(response.content, "Answer.");
    assert!(response.tool_calls.is_empty());

    // Both reasoning deltas and the text delta are forwarded live.
    let evt1 = rx.recv().await.unwrap();
    let evt2 = rx.recv().await.unwrap();
    let evt3 = rx.recv().await.unwrap();
    assert!(matches!(evt1, AgentEvent::ReasoningDelta { content } if content == "Let me think"));
    assert!(matches!(evt2, AgentEvent::ReasoningDelta { content } if content == " about this."));
    assert!(matches!(evt3, AgentEvent::TextDelta { content } if content == "Answer."));
}

#[tokio::test]
async fn tool_call_deltas_assembled_into_tool_calls() {
    let (tx, _rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some("call_1".into()),
            name: Some("fs_read".into()),
            arguments: Some("{\"pat".into()),
        },
        LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: None,
            name: None,
            arguments: Some("h\": \"/tmp\"}".into()),
        },
        LlmStreamEvent::Done {
            finish_reason: "tool_calls".into(),
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    assert_eq!(response.tool_calls.len(), 1);
    let tc = &response.tool_calls[0];
    assert_eq!(tc.id, "call_1");
    assert_eq!(tc.name, "fs_read");
    assert_eq!(tc.arguments["path"], "/tmp");
}

#[tokio::test]
async fn multiple_tool_calls_assembled_by_index() {
    let (tx, _rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some("c0".into()),
            name: Some("tool_a".into()),
            arguments: Some("{}".into()),
        },
        LlmStreamEvent::ToolCallDelta {
            index: 1,
            id: Some("c1".into()),
            name: Some("tool_b".into()),
            arguments: Some("{}".into()),
        },
        LlmStreamEvent::Done {
            finish_reason: "tool_calls".into(),
        },
    ]);

    let response = collect_stream(stream, &tx).await.unwrap();
    assert_eq!(response.tool_calls.len(), 2);
    assert_eq!(response.tool_calls[0].name, "tool_a");
    assert_eq!(response.tool_calls[1].name, "tool_b");
}

#[tokio::test]
async fn zero_event_stream_returns_distinct_error() {
    // A completely empty stream (server immediately closed connection)
    // must fail with a message clearly identifying the zero-event case,
    // distinguishing it from a mid-response truncation.
    let (tx, _rx) = mpsc::channel(16);
    let stream = make_stream(vec![]);
    let err = collect_stream(stream, &tx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("zero events"),
        "zero-event error should mention 'zero events', got: {msg}"
    );
}

#[tokio::test]
async fn truncated_stream_returns_distinct_error() {
    // A stream that started but never sent a Done event should produce an
    // error message about truncation, not about zero events.
    let (tx, _rx) = mpsc::channel(16);
    let stream = make_stream(vec![LlmStreamEvent::TextDelta {
        content: "partial response".into(),
    }]);
    let err = collect_stream(stream, &tx).await.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("truncated"),
        "truncated-stream error should mention 'truncated', got: {msg}"
    );
}

#[tokio::test]
async fn malformed_json_arguments_emit_error_and_fail() {
    // Malformed JSON must hard-fail with a visible AgentEvent::Error so the
    // SSE client always sees why the stream was terminated.
    let (tx, mut rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::ToolCallDelta {
            index: 0,
            id: Some("c1".into()),
            name: Some("do_thing".into()),
            arguments: Some("{ NOT VALID JSON ".into()),
        },
        LlmStreamEvent::Done {
            finish_reason: "tool_calls".into(),
        },
    ]);

    // collect_stream must return Err on malformed JSON.
    assert!(collect_stream(stream, &tx).await.is_err());

    // An AgentEvent::Error must have been sent before the bail.
    drop(tx); // close sender so try_recv can drain
    let events: Vec<_> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    assert!(
        events.iter().any(|e| matches!(e, AgentEvent::Error { .. })),
        "AgentEvent::Error must be emitted for malformed JSON arguments"
    );
}

#[tokio::test]
async fn oversized_tool_call_index_returns_error() {
    // An index >= MAX_TOOL_CALL_INDEX must be rejected immediately to
    // prevent unbounded Vec growth from a malformed or adversarial stream.
    let (tx, _rx) = mpsc::channel(16);
    let stream = make_stream(vec![
        LlmStreamEvent::ToolCallDelta {
            index: MAX_TOOL_CALL_INDEX, // at or beyond the hard limit → rejected
            id: Some("c0".into()),
            name: Some("do_thing".into()),
            arguments: Some("{}".into()),
        },
        LlmStreamEvent::Done {
            finish_reason: "tool_calls".into(),
        },
    ]);

    assert!(collect_stream(stream, &tx).await.is_err());
}
