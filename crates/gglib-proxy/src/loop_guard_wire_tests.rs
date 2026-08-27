//! Unit tests for [`super`]: the permissive wire view.
//!
//! Split out of `loop_guard_wire.rs` to keep that file under the repo's
//! file-size ratchet; see `scripts/check_rust_complexity.sh`.
//!
//! The end-to-end cases run through [`crate::loop_guard::scan_history`] rather
//! than this module's own functions, because the property under test is not
//! "the reader returns an empty vec" — it is "the guard still reaches a
//! verdict". A wire type that fails to deserialize takes the whole envelope
//! with it, and the only observable difference is a `Pass` where a rejection
//! belonged. Asserting on `domain_calls` alone would miss exactly that.

use super::*;
use crate::loop_guard::{LoopGuardConfig, LoopGuardVerdict, scan_history};
use gglib_core::Settings;
use serde_json::json;

fn cfg() -> LoopGuardConfig {
    LoopGuardConfig::from_settings(&Settings::with_defaults()).expect("guard on by default")
}

fn body(messages: &[Value]) -> Vec<u8> {
    json!({ "model": "m", "messages": messages })
        .to_string()
        .into_bytes()
}

/// Three identical *mutating* batches trip `max_repeated_batch_steps` (2).
/// Preceded by one assistant message carrying `odd`, whichever shape that is.
fn history_after(odd: Value) -> Vec<Value> {
    let mut msgs = vec![odd];
    for _ in 0..3 {
        msgs.push(json!({
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": "c1",
                "type": "function",
                "function": { "name": "write_file", "arguments": r#"{"path":"a.rs"}"# }
            }]
        }));
    }
    msgs
}

/// The control. Without an odd message in front, this history is a loop — so
/// every case below that returns `Pass` is the odd shape disabling the guard,
/// not a history that was never looping.
#[test]
fn the_control_history_is_a_loop() {
    let msgs = history_after(json!({ "role": "user", "content": "go" }));
    assert!(
        matches!(
            scan_history(&body(&msgs), &cfg()).verdict,
            LoopGuardVerdict::LoopDetected { .. }
        ),
        "precondition: the shared history must trip on its own"
    );
}

/// The shape that made this a bug rather than a theory.
///
/// Anything serialising an assistant message from a struct whose `tool_calls`
/// is an `Optional[list]` emits `null` when there were none — the OpenAI Python
/// SDK's `model_dump()`, LiteLLM, LangChain. Typed as `Vec<WireToolCall>` this
/// failed the envelope, and `scan_history`'s fail-open arm then returned `Pass`
/// for the request. A replayed history only grows, so it returned `Pass` for
/// every later turn of that conversation too.
#[test]
fn a_null_tool_calls_does_not_disable_the_guard() {
    let msgs = history_after(json!({
        "role": "assistant", "content": "on it", "tool_calls": null
    }));
    assert!(
        matches!(
            scan_history(&body(&msgs), &cfg()).verdict,
            LoopGuardVerdict::LoopDetected { .. }
        ),
        "a null tool_calls must not switch the guard off"
    );
}

/// One level down, and the field #923 missed while fixing its sibling.
/// `arguments` was already a `Value` for exactly this reason; `function` was
/// still a struct, so a present-but-not-an-object value failed the envelope.
#[test]
fn a_non_object_function_does_not_disable_the_guard() {
    let msgs = history_after(json!({
        "role": "assistant",
        "content": null,
        "tool_calls": [{ "id": "x", "type": "function", "function": "search" }]
    }));
    assert!(
        matches!(
            scan_history(&body(&msgs), &cfg()).verdict,
            LoopGuardVerdict::LoopDetected { .. }
        ),
        "a non-object function must not switch the guard off"
    );
}

/// An array whose elements are not calls. `Vec<WireToolCall>` failed on this
/// too, since the element type was a struct.
#[test]
fn a_tool_calls_array_of_junk_does_not_disable_the_guard() {
    let msgs = history_after(json!({
        "role": "assistant", "content": null, "tool_calls": [7, null]
    }));
    assert!(
        matches!(
            scan_history(&body(&msgs), &cfg()).verdict,
            LoopGuardVerdict::LoopDetected { .. }
        ),
        "junk inside tool_calls must not switch the guard off"
    );
}

/// `{}` rather than `[]` — the remaining container confusion.
#[test]
fn an_object_tool_calls_does_not_disable_the_guard() {
    let msgs = history_after(json!({
        "role": "assistant", "content": null, "tool_calls": {}
    }));
    assert!(
        matches!(
            scan_history(&body(&msgs), &cfg()).verdict,
            LoopGuardVerdict::LoopDetected { .. }
        ),
        "an object tool_calls must not switch the guard off"
    );
}

// ── The reader, directly ──────────────────────────────────────────────────

/// Every non-array shape reads as "no tool calls", which the caller treats as
/// a prose turn. The alternative — inventing a call — would put a batch into
/// the detector's run that the model never made.
#[test]
fn every_non_array_shape_reads_as_no_calls() {
    for shape in [
        Value::Null,
        json!("write_file"),
        json!({}),
        json!(7),
        json!(true),
    ] {
        assert!(
            domain_calls(&shape).is_empty(),
            "expected no calls from {shape}"
        );
    }
}

/// A well-formed call still reads exactly as it did before the fields became
/// `Value`s — this is the behaviour the loosening must not have changed.
#[test]
fn a_well_formed_call_reads_unchanged() {
    let calls = domain_calls(&json!([{
        "id": "c1",
        "type": "function",
        "function": { "name": "read_file", "arguments": r#"{"path":"a.rs"}"# }
    }]));
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].id, "c1");
    assert_eq!(calls[0].name, "read_file");
    assert_eq!(calls[0].arguments, json!({ "path": "a.rs" }));
}

/// The documented deviation, kept: a bare object where the wire format says a
/// JSON-encoded string.
#[test]
fn bare_object_arguments_are_read_as_given() {
    let calls = domain_calls(&json!([{
        "function": { "name": "read_file", "arguments": { "path": "a.rs" } }
    }]));
    assert_eq!(calls[0].arguments, json!({ "path": "a.rs" }));
}

/// Malformed argument strings hash as themselves, so two identical malformed
/// batches still count as a repeat.
#[test]
fn malformed_arguments_fall_back_to_the_raw_string() {
    let calls = domain_calls(&json!([{
        "function": { "name": "read_file", "arguments": "{not json" }
    }]));
    assert_eq!(calls[0].arguments, json!("{not json"));
}

/// A call missing `function` entirely is still a call — it takes part in the
/// batch signature as an unnamed one rather than vanishing, which would let a
/// repeated malformed batch escape by looking like a prose turn.
#[test]
fn a_call_without_a_function_is_still_a_call() {
    let calls = domain_calls(&json!([{ "id": "c1" }]));
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "");
    assert_eq!(calls[0].arguments, Value::Null);
}

/// Non-string `id` and `name` read as empty rather than failing, which is what
/// keeps them off the envelope's failure surface.
#[test]
fn non_string_id_and_name_read_as_empty() {
    let calls = domain_calls(&json!([{
        "id": 42, "function": { "name": ["read_file"], "arguments": "{}" }
    }]));
    assert_eq!(calls[0].id, "");
    assert_eq!(calls[0].name, "");
}
