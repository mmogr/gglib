//! A model's reply as the compare run reads it off the wire.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Utc;
use gglib_core::domain::benchmark::{BenchmarkEvent, CompareConfig, ModelCompareResult};
use gglib_core::domain::{Model, NewModel};
use gglib_core::sse::DataFrames;
use serde_json::{Value, json};

use super::{ReplyFold, build_compare_request_body, reply_chunks};
use crate::benchmark::mapper::{extract_text_delta, extract_usage};

/// One streamed chunk of a reply, carrying `text`.
fn delta(text: &str) -> String {
    let chunk = json!({ "choices": [{ "delta": { "content": text } }] });
    format!("data: {chunk}\n\n")
}

/// The text of every chunk read off `reads`.
fn text_of(reads: &[&[u8]]) -> Vec<String> {
    let mut frames = DataFrames::unbounded();
    reads
        .iter()
        .flat_map(|read| reply_chunks(&mut frames, read, "one"))
        .filter_map(|chunk| extract_text_delta(&chunk))
        .collect()
}

#[test]
fn a_reply_split_inside_a_character_reaches_its_chunks_whole() {
    let wire = format!(
        "{}{}data: [DONE]\n\n",
        delta("caf\u{e9}"),
        delta("\u{6a21}\u{578b}")
    );
    let bytes = wire.as_bytes();
    let inside_a_character: Vec<usize> = (1..bytes.len())
        .filter(|cut| !wire.is_char_boundary(*cut))
        .collect();
    assert_eq!(
        inside_a_character.len(),
        5,
        "one cut inside the two-byte character, two inside each three-byte one"
    );

    for cut in inside_a_character {
        assert_eq!(
            text_of(&[&bytes[..cut], &bytes[cut..]]),
            ["caf\u{e9}", "\u{6a21}\u{578b}"],
            "cut at byte {cut}"
        );
    }
}

#[test]
fn a_payload_that_is_not_json_is_skipped_and_the_next_chunk_read() {
    let wire = format!("data: {{\"choices\"\n\n{}", delta("one"));

    assert_eq!(text_of(&[wire.as_bytes()]), ["one"]);
}

fn config() -> CompareConfig {
    CompareConfig {
        model_ids: vec![1],
        prompt: "Write a haiku about a lighthouse.".into(),
        system_prompt: None,
        inference: None,
        ctx_size: None,
    }
}

fn reasoning(text: &str) -> Value {
    json!({ "choices": [{ "delta": { "content": null, "reasoning_content": text } }] })
}

fn answer(text: &str) -> Value {
    json!({ "choices": [{ "delta": { "content": text } }] })
}

/// Fold `chunks`, each arriving at its offset in milliseconds from one start,
/// then end the stream at `end_ms`: every event sent, and the record kept.
fn fold(chunks: &[(u64, Value)], end_ms: u64) -> (Vec<BenchmarkEvent>, ModelCompareResult) {
    let t0 = Instant::now();
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    let mut reply = ReplyFold::new(1);
    let mut events: Vec<BenchmarkEvent> = chunks
        .iter()
        .flat_map(|(ms, chunk)| reply.absorb(chunk, at(*ms)))
        .collect();
    events.extend(reply.finish(at(end_ms)));
    (events, reply.into_result(&config(), 7))
}

/// `"thinking"`, `"thought 4.1"` or `"text …"` for each event, so a sequence
/// reads as one assertion.
fn describe(events: &[BenchmarkEvent]) -> Vec<String> {
    events
        .iter()
        .map(|event| match event {
            BenchmarkEvent::ModelThinking { seconds: None, .. } => "thinking".to_owned(),
            BenchmarkEvent::ModelThinking {
                seconds: Some(s), ..
            } => format!("thought {s:.3}"),
            BenchmarkEvent::ModelTextDelta { text, .. } => format!("text {text}"),
            other => format!("{other:?}"),
        })
        .collect()
}

/// A thinking model's reply as llama-server streams it: reasoning, then the
/// answer. The answer alone is kept and streamed, and the reasoning shows as
/// its start and its length, the length before the answer's first text.
#[test]
fn a_thinking_reply_keeps_its_reasoning_out_of_the_answer() {
    let (events, result) = fold(
        &[
            (0, reasoning("We need to respond")),
            (1_000, reasoning(" with a haiku.")),
            (4_100, answer("Stone sentinel stands")),
            (4_200, answer("\nits beam cuts the sea")),
            (
                4_300,
                json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] }),
            ),
        ],
        4_400,
    );

    assert_eq!(
        describe(&events),
        [
            "thinking",
            "thought 4.100",
            "text Stone sentinel stands",
            "text \nits beam cuts the sea",
        ]
    );
    assert_eq!(
        result.response_text,
        "Stone sentinel stands\nits beam cuts the sea"
    );
    assert!(!result.was_truncated);
}

/// A reply that reasons and never answers still says how long it thought,
/// timed to the end of the stream.
#[test]
fn a_reply_that_never_answers_is_timed_to_the_end_of_the_stream() {
    let (events, result) = fold(&[(0, reasoning("Hmm.")), (900, reasoning(" Hmm."))], 2_500);

    assert_eq!(describe(&events), ["thinking", "thought 2.500"]);
    assert_eq!(result.response_text, "");
}

/// A reply is cut short when the stream breaks off before a `finish_reason`,
/// not when its answer is empty: a thinking model that spends its reply on
/// reasoning and stops has finished, and one cut off mid-answer has not.
#[test]
fn a_reply_is_truncated_when_it_never_finishes() {
    let stop = json!({ "choices": [{ "delta": {}, "finish_reason": "stop" }] });
    let length = json!({ "choices": [{ "delta": {}, "finish_reason": "length" }] });

    let (_, thought_and_stopped) = fold(&[(0, reasoning("Hmm.")), (10, stop)], 20);
    let (_, broke_off) = fold(&[(0, answer("Stone"))], 20);
    let (_, hit_the_limit) = fold(&[(0, answer("Stone")), (10, length)], 20);

    assert!(!thought_and_stopped.was_truncated);
    assert!(broke_off.was_truncated);
    assert!(hit_the_limit.was_truncated);
}

/// A model that does not think says nothing about thinking, and reasoning
/// after the answer has begun is not a second thinking phase.
#[test]
fn only_reasoning_before_the_answer_is_timed() {
    let (events, _) = fold(&[(0, answer("Hello")), (500, reasoning("late"))], 1_000);

    assert_eq!(describe(&events), ["text Hello"]);
}

/// A reply without `usage` takes its counts from llama-server's `timings`.
#[test]
fn the_token_counts_come_from_timings_when_usage_is_absent() {
    let last = json!({
        "choices": [{ "delta": {}, "finish_reason": "stop" }],
        "timings": { "prompt_n": 12, "predicted_n": 34, "predicted_per_second": 27.9 }
    });
    let (_, result) = fold(&[(0, answer("Hi")), (10, last)], 20);

    assert_eq!(result.prompt_tokens, Some(12));
    assert_eq!(result.completion_tokens, Some(34));
}

/// Where a chunk carries both, `usage` is the count.
#[test]
fn usage_wins_over_timings() {
    let chunk = json!({
        "choices": [],
        "usage": { "prompt_tokens": 10, "completion_tokens": 100 },
        "timings": { "prompt_n": 99, "predicted_n": 999 }
    });

    assert_eq!(extract_usage(&chunk), (Some(10), Some(100)));
}

/// A streamed chat completion carries `usage` only when the request asks.
#[test]
fn the_request_asks_for_usage() {
    let new = NewModel::new("one".into(), PathBuf::from("/one.gguf"), 1.0, Utc::now());
    let body = build_compare_request_body(&config(), &Model::stored(1, &new), None);

    assert_eq!(body["stream"], json!(true));
    assert_eq!(body["stream_options"]["include_usage"], json!(true));
}
