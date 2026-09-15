//! Tests for [`super::elide`], and for [`truncate_history`] over the array
//! form of `content`, the shape VS Code's LLM gateway sends.

use serde_json::{Value, json};

use super::super::truncation::{
    PROTECTED_TAIL_COUNT, TOOL_CONTENT_THRESHOLD_CHARS, TRUNCATION_PLACEHOLDER, truncate_history,
};
use super::elide;

/// A tool result in the array form, one text part, as the gateway sends it.
fn part_msg(text: &str) -> Value {
    json!({"role": "tool", "tool_call_id": "call_0", "content": [{"type": "text", "text": text}]})
}

fn body(messages: &[Value]) -> Value {
    json!({"model": "test-model", "messages": messages})
}

fn big(n: usize) -> String {
    "x".repeat(n)
}

/// Pad a message list out past [`PROTECTED_TAIL_COUNT`] so the leading entries
/// are actually eligible for trimming.
fn with_tail(mut messages: Vec<Value>) -> Vec<Value> {
    for _ in 0..PROTECTED_TAIL_COUNT {
        messages.push(json!({"role": "user", "content": "ok"}));
    }
    messages
}

/// A budget with room to spare for every fixture here.
const ROOMY: usize = 240_000;

#[test]
fn an_array_form_tool_result_is_truncated_rather_than_skipped() {
    // The string-form fixture from `truncation_tests`, in the array form:
    // four 100k tool results outside the protected tail, and the quantized
    // watermark target takes the three oldest.
    let mut b = body(&with_tail(vec![part_msg(&big(100_000)); 4]));

    let report = truncate_history(&mut b, ROOMY).unwrap();

    assert_eq!(
        report.messages_truncated, 3,
        "as many oldest messages as the watermark target needs"
    );
    let parts = b["messages"][0]["content"].as_array().unwrap();
    assert_eq!(parts.len(), 1, "the array shape is kept");
    assert_eq!(parts[0]["type"], "text");
    assert_eq!(parts[0]["text"], TRUNCATION_PLACEHOLDER);
    assert_eq!(
        b["messages"][3]["content"][0]["text"]
            .as_str()
            .unwrap()
            .len(),
        100_000,
        "newest big message preserved once the target is met"
    );
}

#[test]
fn a_long_array_form_history_is_truncated_instead_of_refused() {
    // One oversized array-form tool result over a budget it alone exceeds.
    // Before, the message was skipped and the request refused as over budget
    // with nothing elided.
    let mut b = body(&with_tail(vec![part_msg(&big(100_000))]));

    let report = truncate_history(&mut b, 50_000).expect("elided, not refused");

    assert_eq!(report.messages_truncated, 1);
    assert!(report.payload_chars_after <= 50_000);
}

#[test]
fn a_part_that_is_not_text_survives_truncation() {
    let image = json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,AAAA"}});
    let mut msg = json!({"role": "tool", "tool_call_id": "call_0", "content": [
        {"type": "text", "text": big(TOOL_CONTENT_THRESHOLD_CHARS + 1)},
        image,
    ]});

    let reclaimed = elide(&mut msg).expect("over the threshold");

    assert_eq!(
        msg["content"][0],
        json!({"type": "text", "text": TRUNCATION_PLACEHOLDER})
    );
    assert_eq!(msg["content"][1], image, "kept, in its place");
    assert_eq!(
        reclaimed,
        TOOL_CONTENT_THRESHOLD_CHARS + 1 - TRUNCATION_PLACEHOLDER.len()
    );
}

#[test]
fn a_string_is_elided_the_way_it_always_was() {
    let mut msg = json!({"role": "tool", "content": big(3_000)});

    assert_eq!(elide(&mut msg), Some(3_000 - TRUNCATION_PLACEHOLDER.len()));
    assert_eq!(msg["content"], TRUNCATION_PLACEHOLDER);
}

#[test]
fn under_the_threshold_or_without_content_nothing_is_elided() {
    let mut short = json!({"role": "tool", "content": big(TOOL_CONTENT_THRESHOLD_CHARS)});
    let before = short.clone();
    assert_eq!(elide(&mut short), None, "at the threshold is not over it");
    assert_eq!(short, before);

    let mut calls = json!({"role": "assistant", "tool_calls": [
        {"id": "c", "type": "function", "function": {"name": "f", "arguments": "{}"}}
    ]});
    let before = calls.clone();
    assert_eq!(elide(&mut calls), None);
    assert_eq!(before, calls, "tool_calls at any role are never touched");
}

/// Thirty short parts add up past the threshold, but thirty placeholders
/// would be longer than the text: the message is left as it is rather than
/// grown.
#[test]
fn a_message_of_many_short_parts_is_left_alone_rather_than_grown() {
    let parts: Vec<Value> = (0..30)
        .map(|_| json!({"type": "text", "text": big(71)}))
        .collect();
    let mut msg = json!({"role": "tool", "content": parts});
    let before = msg.clone();

    assert_eq!(
        elide(&mut msg),
        None,
        "2,130 characters, but 30 placeholders are 3,000"
    );
    assert_eq!(msg, before);
}

#[test]
fn several_text_parts_each_carry_the_placeholder() {
    let mut msg = json!({"role": "tool", "content": [
        {"type": "text", "text": big(1_500)},
        {"type": "text", "text": big(1_500)},
    ]});

    let reclaimed = elide(&mut msg).expect("3,000 characters of text, over the threshold together");

    assert_eq!(msg["content"][0]["text"], TRUNCATION_PLACEHOLDER);
    assert_eq!(msg["content"][1]["text"], TRUNCATION_PLACEHOLDER);
    assert_eq!(reclaimed, 3_000 - 2 * TRUNCATION_PLACEHOLDER.len());
}
