//! A model's reply as the compare run reads it off the wire.

use gglib_core::sse::DataFrames;
use serde_json::json;

use super::reply_chunks;
use crate::benchmark::mapper::extract_text_delta;

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
