//! What the decoder yields for a stream, and how it ends one.

use anyhow::Result;

use super::SseStreamDecoder;
use crate::LlmStreamEvent;

fn text_delta_frame(text: &str) -> String {
    let json = serde_json::json!({
        "choices": [{
            "delta": { "content": text },
            "finish_reason": null
        }]
    });
    format!("data: {json}\n")
}

fn done_frame() -> &'static str {
    "data: [DONE]\n"
}

fn finish_reason_frame() -> String {
    let json = serde_json::json!({
        "choices": [{
            "delta": {},
            "finish_reason": "stop"
        }]
    });
    format!("data: {json}\n")
}

// ---- helpers ------------------------------------------------------------

fn collect_all(decoder: &mut SseStreamDecoder, input: &str) -> (Vec<LlmStreamEvent>, bool) {
    let (raw, stop) = decoder.feed_bytes(input.as_bytes());
    let events: Vec<_> = raw.into_iter().map(Result::unwrap).collect();
    (events, stop)
}

// ---- tests --------------------------------------------------------------

#[test]
fn text_delta_is_emitted() {
    let mut dec = SseStreamDecoder::default();
    let (events, stop) = collect_all(&mut dec, &text_delta_frame("hello"));
    assert!(!stop);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, LlmStreamEvent::TextDelta { content } if content == "hello"))
    );
}

#[test]
fn done_sentinel_signals_stop_and_emits_fallback() {
    let mut dec = SseStreamDecoder::default();
    let (events, stop) = collect_all(&mut dec, done_frame());
    assert!(stop, "decoder should signal stop on [DONE]");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, LlmStreamEvent::Done { .. })),
        "fallback Done should be emitted when no prior finish_reason"
    );
    assert!(
        dec.finish().is_none(),
        "finish() must return None after a [DONE] sentinel — done_sent must be set"
    );
}

/// The relabelling this fix exists to stop. A `[DONE]` arriving with no
/// prior finish chunk means the upstream never said how the turn ended;
/// synthesising `"stop"` there reported a truncation as a clean answer,
/// and made `finish_reason == "length"` unusable as a truncation signal
/// because the abnormal cases were hiding inside `"stop"`.
#[test]
fn a_synthesised_done_reports_an_unknown_reason_not_stop() {
    let mut dec = SseStreamDecoder::default();
    let (events, _) = collect_all(&mut dec, done_frame());
    let done = events
        .iter()
        .find_map(|e| match e {
            LlmStreamEvent::Done { finish_reason } => Some(finish_reason),
            _ => None,
        })
        .expect("a fallback Done is still emitted");
    assert_eq!(*done, None, "must not claim the turn stopped cleanly");
}

#[test]
fn a_byte_stream_that_just_stops_reports_an_unknown_reason() {
    let mut dec = SseStreamDecoder::default();
    let _ = collect_all(&mut dec, &text_delta_frame("partial"));
    match dec.finish() {
        Some(LlmStreamEvent::Done { finish_reason }) => {
            assert_eq!(finish_reason, None);
        }
        other => panic!("expected a fallback Done, got {other:?}"),
    }
}

/// A reason the upstream actually reported is passed through untouched —
/// the fix must not lose real information while refusing to invent it.
#[test]
fn a_reported_finish_reason_survives_intact() {
    let mut dec = SseStreamDecoder::default();
    let (events, _) = collect_all(&mut dec, &finish_reason_frame());
    let done = events
        .iter()
        .find_map(|e| match e {
            LlmStreamEvent::Done { finish_reason } => Some(finish_reason),
            _ => None,
        })
        .expect("Done from the reported finish_reason");
    assert!(done.is_some(), "a real reason must not be discarded");
}

#[test]
fn finish_reason_then_done_no_duplicate_done() {
    let mut dec = SseStreamDecoder::default();
    let input = format!("{}{}", finish_reason_frame(), done_frame());
    let (events, stop) = collect_all(&mut dec, &input);
    assert!(stop);
    let done_count = events
        .iter()
        .filter(|e| matches!(e, LlmStreamEvent::Done { .. }))
        .count();
    assert_eq!(done_count, 1, "exactly one Done should be emitted");
}

#[test]
fn finish_emits_fallback_when_stream_ends_without_done() {
    let mut dec = SseStreamDecoder::default();
    let _ = collect_all(&mut dec, &text_delta_frame("partial"));
    let fallback = dec.finish();
    assert!(
        fallback.is_some(),
        "finish() should return a fallback Done when stream ends without one"
    );
}

#[test]
fn finish_returns_none_when_done_already_sent() {
    let mut dec = SseStreamDecoder::default();
    let _ = collect_all(&mut dec, &finish_reason_frame());
    assert!(
        dec.finish().is_none(),
        "finish() must not emit a second Done"
    );
}

#[test]
fn inline_error_frame_signals_stop_and_suppresses_fallback_done() {
    let mut dec = SseStreamDecoder::default();
    let frame = format!(
        "data: {}\n",
        serde_json::json!({ "error": { "message": "boom" } })
    );
    let (events, stop) = collect_all(&mut dec, &frame);
    assert!(stop, "inline error frame should signal stop");
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], LlmStreamEvent::UpstreamError { .. }));
    assert!(
        dec.finish().is_none(),
        "finish() must not append a fallback Done after an inline error"
    );
}

#[test]
fn partial_line_buffered_until_newline_arrives() {
    let mut dec = SseStreamDecoder::default();
    let full_frame = text_delta_frame("world");

    let mid = full_frame.len() / 2;
    let (first_events, stop1) = collect_all(&mut dec, &full_frame[..mid]);
    assert!(!stop1);
    assert!(first_events.is_empty(), "no complete line yet");

    let (second_events, stop2) = collect_all(&mut dec, &full_frame[mid..]);
    assert!(!stop2);
    assert!(
        second_events
            .iter()
            .any(|e| matches!(e, LlmStreamEvent::TextDelta { .. })),
        "TextDelta should be emitted once the newline arrives"
    );
}

/// A stream the pinned llama-server sent for gglib's own request body
/// (`return_progress`, `include_usage`), as the recorder kept it in
/// `contracts/upstream/llama/bodies/`: progress first, then deltas, one
/// finish reason, usage with the cached count, and the sentinel.
#[test]
fn a_recorded_llama_server_stream_decodes_end_to_end() {
    let body = include_str!("../../../../contracts/upstream/llama/bodies/stream.sse");
    let mut dec = SseStreamDecoder::default();
    let (events, stop) = collect_all(&mut dec, body);
    assert!(stop, "[DONE] ends it");
    let progress: Vec<(u32, u32, u32)> = events
        .iter()
        .filter_map(|e| match e {
            LlmStreamEvent::PromptProgress {
                processed,
                total,
                cached,
                ..
            } => Some((*processed, *total, *cached)),
            _ => None,
        })
        .collect();
    assert!(
        matches!(events.first(), Some(LlmStreamEvent::PromptProgress { .. })),
        "{events:?}"
    );
    // The same frames as the committed body's own JSON has them, read
    // without the parser: every field present, and the last frame has the
    // whole prompt in.
    let frames: Vec<serde_json::Value> = body
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .filter(|data| data.starts_with('{'))
        .map(|data| serde_json::from_str(data).expect("a JSON frame"))
        .collect();
    let number = |value: &serde_json::Value| {
        u32::try_from(value.as_u64().expect("a recorded count")).expect("fits")
    };
    let raw_progress: Vec<(u32, u32, u32)> = frames
        .iter()
        .filter_map(|frame| frame.get("prompt_progress"))
        .map(|p| {
            (
                number(&p["processed"]),
                number(&p["total"]),
                number(&p["cache"]),
            )
        })
        .collect();
    assert_eq!(progress, raw_progress);
    let last = raw_progress.last().expect("a progress frame");
    assert!(last.1 > 0 && last.0 == last.1, "{raw_progress:?}");
    assert!(events.iter().any(|e| matches!(
        e,
        LlmStreamEvent::TextDelta { .. } | LlmStreamEvent::ReasoningDelta { .. }
    )));
    let done: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            LlmStreamEvent::Done { finish_reason } => Some(finish_reason),
            _ => None,
        })
        .collect();
    assert!(matches!(done[..], [Some(_)]), "{events:?}");
    let usage = events.iter().find_map(|e| match e {
        LlmStreamEvent::Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cached_tokens,
        } => Some((
            *prompt_tokens,
            *completion_tokens,
            *total_tokens,
            *cached_tokens,
        )),
        _ => None,
    });
    let raw = frames
        .iter()
        .find_map(|frame| frame.get("usage"))
        .expect("a usage frame");
    let raw_usage = (
        number(&raw["prompt_tokens"]),
        number(&raw["completion_tokens"]),
        number(&raw["total_tokens"]),
        Some(number(&raw["prompt_tokens_details"]["cached_tokens"])),
    );
    assert_eq!(usage, Some(raw_usage), "{events:?}");
}
