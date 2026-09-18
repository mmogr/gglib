//! Tests for the bound on what the note echoes back into the prompt.
//!
//! Their own file, apart from the note's shape tests: this is about the one
//! part of the note a client controls. The batch signature carries each tool
//! *name* verbatim off the wire — the arguments are hashed, the name is not —
//! and under `note` that string goes into the model's prompt rather than into
//! an error body the client reads back.

use super::{LoopGuardNote, SIGNATURE_LIMIT};
use crate::loop_guard::LoopGuardVerdict;

#[test]
fn a_long_tool_name_is_bounded_before_it_reaches_the_prompt() {
    // The signature carries the client's own tool names verbatim, so it is the
    // one part of the note a client controls. Under `refuse` it went into an
    // error body; under `note` it goes into the prompt.
    let long = "z".repeat(SIGNATURE_LIMIT * 20);
    let note = LoopGuardNote::for_verdict(&LoopGuardVerdict::LoopDetected {
        signature: format!("{long}:00000000deadbeef"),
    })
    .expect("a tripped verdict has a note");

    assert!(
        note.text().chars().count() < SIGNATURE_LIMIT * 3,
        "the note must not be dominated by a tool name: {} chars",
        note.text().chars().count()
    );
    assert!(note.text().contains('…'), "the truncation is visible");
    assert!(note.text().starts_with("[gglib loop guard]"));
}

#[test]
fn a_signature_inside_the_bound_is_echoed_whole() {
    // The bound must not cost the ordinary case its readability: "you keep
    // calling `write_file`" is the whole point of naming the batch.
    let signature = "write_file:1f2e3d4c|read_file:00abcdef";
    let note = LoopGuardNote::for_verdict(&LoopGuardVerdict::LoopDetected {
        signature: signature.into(),
    })
    .expect("a tripped verdict has a note");

    assert!(note.text().contains(signature), "{}", note.text());
    assert!(!note.text().contains('…'));
}

#[test]
fn the_bound_falls_on_a_character_boundary() {
    // A byte-slice at the limit would panic here, or emit a partial code
    // point into the prompt. Multi-byte throughout, so it lands mid-character.
    let signature = "café☃".repeat(200);
    let note = LoopGuardNote::for_verdict(&LoopGuardVerdict::LoopDetected {
        signature: signature.clone(),
    })
    .expect("a tripped verdict has a note");

    let text = note.text();
    assert!(text.contains('…'), "a signature this long is truncated");
    // The proof is that it is valid UTF-8 at all — `text()` returns `&str`, so
    // a split code point could not have been built. Pin the count too.
    let kept: String = signature.chars().take(SIGNATURE_LIMIT).collect();
    assert!(
        text.contains(&kept),
        "the first SIGNATURE_LIMIT characters survive"
    );
}
