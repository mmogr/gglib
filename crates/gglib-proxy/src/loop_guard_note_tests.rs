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

/// The history a tripped request has, with `n` identical `write_file` batches.
fn looping(n: usize) -> Vec<Value> {
    (0..n)
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
        .collect()
}

fn scan(body: &Bytes) -> LoopGuardVerdict {
    let cfg =
        crate::loop_guard::LoopGuardConfig::from_settings(&gglib_core::Settings::with_defaults())
            .expect("the guard is on by default");
    crate::loop_guard::scan_history(body, &cfg).verdict
}

#[test]
fn the_note_neither_creates_a_trip_nor_masks_one() {
    // Deliberately *not* "append the note to a body that already trips and
    // check the verdict did not change": `scan_history` returns on the first
    // trip it finds, so such a test never reaches the note and holds whatever
    // the note is. Both halves below scan a body whose verdict the note could
    // actually move.
    let note = loop_note();

    // One repeat under the threshold: passes, and must still pass with the
    // note in it. A note that added a tool-call batch would trip here.
    let under = body(json!(looping(2)));
    assert_eq!(
        scan(&under),
        LoopGuardVerdict::Pass,
        "the fixture must pass"
    );
    assert_eq!(
        scan(&note.append_to(under)),
        LoopGuardVerdict::Pass,
        "the note must not create a trip"
    );

    // One more repeat: trips, and must still trip with the note in it.
    //
    // This half is weaker than it looks and is kept for the arithmetic rather
    // than the hazard: `scan_history` returns at the first trip it finds, so a
    // note appended after that point is never reached, and a delivery that
    // *did* reset the detectors would not fail here. The first half above is
    // the one that can fail.
    let over = body(json!(looping(3)));
    let verdict = scan(&over);
    assert!(
        matches!(verdict, LoopGuardVerdict::LoopDetected { .. }),
        "the fixture must trip: {verdict:?}"
    );
    assert_eq!(
        scan(&note.append_to(over)),
        verdict,
        "the note must not mask a trip"
    );
}

#[test]
fn the_note_survives_a_truncation_that_trims_the_history_around_it() {
    // A long conversation whose earlier tool results are big enough that the
    // budget forces a trim, with the note appended to the last message.
    //
    // What this pins is the outcome, not the mechanism: the trim runs oldest
    // to newest and stops at the low watermark long before it reaches the
    // tail, so forcing `is_tail_protected` false leaves this green. The
    // protected-tail guarantee is real — the note is always at index
    // `total - 1` — but it is not what this test exercises, and a mutation of
    // that constant would survive.
    let filler = "y".repeat(4_000);
    let mut history = vec![json!({ "role": "system", "content": "be helpful" })];
    for _ in 0..12 {
        history.push(json!({ "role": "assistant", "content": "working on it" }));
        history.push(json!({ "role": "tool", "tool_call_id": "c1", "content": filler }));
    }
    history.push(json!({ "role": "user", "content": "continue" }));

    let note = loop_note();
    let noted = note.append_to(body(json!(history)));
    let mut value: Value = serde_json::from_slice(&noted).expect("json");

    let before = serde_json::to_string(&value).expect("serialise").len();
    // Half the payload: enough to force a trim of the unprotected head, and
    // still room for the protected tail, which truncation may not touch and
    // which is where the note lives.
    let report = gglib_core::request_pipeline::truncate_history(&mut value, before / 2)
        .expect("a conversation this shape can be trimmed to fit");
    assert!(
        report.messages_truncated > 0,
        "the fixture must actually trim something: {report:?}"
    );

    let last = value["messages"]
        .as_array()
        .and_then(|m| m.last())
        .and_then(|m| m["content"].as_str())
        .expect("string content");
    assert!(
        last.ends_with(note.text()),
        "the note must survive the trim, last: {last}"
    );
}
