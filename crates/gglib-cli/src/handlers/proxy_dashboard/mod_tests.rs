//! The dashboard's stream as it is read off the wire.

use gglib_core::sse::DataFrames;
use serde_json::json;

use super::snapshots;

/// One snapshot event, with `model` generating.
fn wire(model: &str) -> String {
    let snapshot = json!({
        "active_connections": [
            { "model_name": model, "started_at_secs": 1, "phase": "generating" }
        ],
        "slots_available": false,
        "total_requests": 1
    });
    format!("data: {snapshot}\n\n")
}

/// The models generating in every snapshot read off `reads`.
fn models_of(reads: &[&[u8]]) -> Vec<String> {
    let mut frames = DataFrames::unbounded();
    reads
        .iter()
        .flat_map(|read| snapshots(&mut frames, read))
        .flat_map(|snapshot| snapshot.active_connections)
        .map(|connection| connection.model_name)
        .collect()
}

#[test]
fn a_model_name_split_across_two_reads_reaches_the_snapshot_whole() {
    let name = "caf\u{e9}-\u{6a21}\u{578b}";
    let wire = format!(": keep-alive\n\n{}", wire(name));
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
            models_of(&[&bytes[..cut], &bytes[cut..]]),
            [name],
            "cut at byte {cut}"
        );
    }
}

#[test]
fn a_payload_that_is_not_a_snapshot_is_skipped_and_the_next_one_drawn() {
    let wire = format!("data: {{\"total_requests\":\"many\"}}\n\n{}", wire("one"));

    assert_eq!(models_of(&[wire.as_bytes()]), ["one"]);
}
