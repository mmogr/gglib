//! Tests for [`super::LoopGuardNote`].
//!
//! The note's whole contract is "the last thing the model reads, marked as
//! gglib's, and nothing else about the request changed". Each test here pins
//! one half of that, across the three shapes a message's `content` takes.

use bytes::Bytes;
use serde_json::{Value, json};

use super::{LoopGuardNote, MARKER};
use crate::loop_guard::LoopGuardVerdict;

fn loop_note() -> LoopGuardNote {
    LoopGuardNote::for_verdict(&LoopGuardVerdict::LoopDetected {
        signature: "write_file:1f2e3d4c".into(),
    })
    .expect("a tripped verdict has a note")
}

fn body(messages: Value) -> Bytes {
    Bytes::from(
        json!({
            "model": "test-model",
            "stream": true,
            "temperature": 0.7,
            "messages": messages,
        })
        .to_string(),
    )
}

fn parsed(bytes: &Bytes) -> Value {
    serde_json::from_slice(bytes).expect("the forwarded body is still JSON")
}

#[test]
fn a_passing_verdict_has_no_note() {
    assert!(LoopGuardNote::for_verdict(&LoopGuardVerdict::Pass).is_none());
}

#[test]
fn the_note_names_the_repeated_batch_and_marks_itself() {
    let text = loop_note().text().to_owned();
    // The literal, not the constant: `starts_with(MARKER)` is true of every
    // string when `MARKER` is empty, so it cannot notice the marker going
    // missing — which is the one thing that tells a model whose words these
    // are.
    assert!(text.starts_with("[gglib loop guard]"), "{text}");
    assert_eq!(MARKER, "[gglib loop guard]");
    assert!(text.contains("write_file:1f2e3d4c"), "{text}");
}

#[test]
fn the_stagnation_note_names_the_count_and_the_limit() {
    let note = LoopGuardNote::for_verdict(&LoopGuardVerdict::StagnationDetected {
        count: 6,
        max_steps: 5,
    })
    .expect("a tripped verdict has a note");
    let text = note.text();
    assert!(text.starts_with("[gglib loop guard]"), "{text}");
    assert!(text.contains('6') && text.contains("limit of 5"), "{text}");
}

#[test]
fn a_string_tail_gains_the_note_last_and_keeps_every_earlier_message() {
    let before = body(json!([
        { "role": "system", "content": "be helpful" },
        { "role": "user", "content": "carry on" },
        { "role": "tool", "tool_call_id": "c1", "content": "1 file changed" },
    ]));
    let note = loop_note();

    let after = parsed(&note.append_to(before.clone()));
    let messages = after["messages"].as_array().expect("messages");

    assert_eq!(messages.len(), 3, "no turn is added to a string tail");
    let tail = messages[2]["content"].as_str().expect("string content");
    assert!(
        tail.starts_with("1 file changed\n\n"),
        "the message keeps its own content first: {tail}"
    );
    assert!(tail.ends_with(note.text()), "the note is last: {tail}");

    // Every earlier message and every other field is unchanged. `Value`
    // equality, not byte equality: the round trip re-sorts object keys,
    // because `serde_json` here is built without `preserve_order`.
    let original = parsed(&before);
    assert_eq!(messages[0], original["messages"][0]);
    assert_eq!(messages[1], original["messages"][1]);
    for field in ["model", "stream", "temperature"] {
        assert_eq!(after[field], original[field], "{field} changed");
    }
}

#[test]
fn an_array_tail_gains_one_text_part_at_the_end() {
    // The shape VS Code's LLM gateway sends on every message.
    let before = body(json!([
        { "role": "user", "content": [
            { "type": "text", "text": "look at this" },
            { "type": "image_url", "image_url": { "url": "data:..." } },
        ] },
    ]));
    let note = loop_note();

    let after = parsed(&note.append_to(before));
    let parts = after["messages"][0]["content"]
        .as_array()
        .expect("the array shape is kept");

    assert_eq!(parts.len(), 3, "exactly one part is added: {parts:?}");
    assert_eq!(
        parts[0]["text"], "look at this",
        "the text part is untouched"
    );
    assert_eq!(
        parts[1]["type"], "image_url",
        "a part that is not text is untouched"
    );
    // As text, not merely present: a part the template renders as prose.
    assert_eq!(parts[2]["type"], "text");
    assert_eq!(parts[2]["text"], note.text());
}

#[test]
fn a_tail_that_cannot_carry_the_note_gets_a_marked_user_turn() {
    // An assistant turn that is only tool calls: `content` is null, so there
    // is nothing to append to.
    let before = body(json!([
        { "role": "user", "content": "go" },
        { "role": "assistant", "content": null, "tool_calls": [
            { "id": "c1", "type": "function",
              "function": { "name": "write_file", "arguments": "{}" } },
        ] },
    ]));
    let note = loop_note();

    let after = parsed(&note.append_to(before));
    let messages = after["messages"].as_array().expect("messages");

    assert_eq!(messages.len(), 3, "a turn is added only in this case");
    assert_eq!(messages[1]["content"], Value::Null, "the tail is untouched");
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"], note.text());
}

#[test]
fn an_empty_string_tail_gains_the_note_with_no_leading_blank_line() {
    let before = body(json!([{ "role": "user", "content": "" }]));
    let note = loop_note();

    let after = parsed(&note.append_to(before));
    assert_eq!(after["messages"][0]["content"], note.text());
}

#[test]
fn a_body_that_cannot_carry_a_note_is_returned_unchanged() {
    let note = loop_note();

    // Not JSON at all.
    let junk = Bytes::from_static(b"not json");
    assert_eq!(note.append_to(junk.clone()), junk);

    // JSON, but no `messages`.
    let no_messages = Bytes::from(json!({ "model": "m" }).to_string());
    assert_eq!(note.append_to(no_messages.clone()), no_messages);

    // `messages`, but empty: there is no last message to append to, and an
    // opening note would be a turn the client never sent.
    let empty = Bytes::from(json!({ "model": "m", "messages": [] }).to_string());
    assert_eq!(note.append_to(empty.clone()), empty);
}

#[test]
fn the_note_is_not_scanned_by_the_guard_that_produced_it() {
    // A tripped history: scan it, note it, then scan the *forwarded* body and
    // assert the verdict is unchanged. The note adds no tool-call batch and
    // no assistant turn, so it can neither trip the guard nor — which is the
    // sharper risk — reset the detectors by looking like a turn boundary.
    let history: Vec<Value> = (0..3)
        .flat_map(|_| {
            vec![
                json!({ "role": "assistant", "content": null, "tool_calls": [
                    { "id": "c1", "type": "function",
                      "function": { "name": "write_file",
                                    "arguments": "{\"path\":\"src/main.rs\"}" } },
                ] }),
                json!({ "role": "tool", "tool_call_id": "c1", "content": "1 file changed" }),
            ]
        })
        .collect();
    let before = body(json!(history));

    let cfg =
        crate::loop_guard::LoopGuardConfig::from_settings(&gglib_core::Settings::with_defaults())
            .expect("the guard is on by default");
    let first = crate::loop_guard::scan_history(&before, &cfg);
    let note = LoopGuardNote::for_verdict(&first.verdict).expect("this history trips");

    let after = note.append_to(before);
    let second = crate::loop_guard::scan_history(&after, &cfg);

    assert_eq!(
        second.verdict, first.verdict,
        "the note must not change what the guard sees"
    );
}
