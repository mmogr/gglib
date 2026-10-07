//! A stream of JSON events as the CLI reads it.

use futures_util::stream;
use gglib_core::domain::benchmark::BenchmarkEvent;

use super::read_json;

/// The events read off `reads`.
async fn events_of(reads: Vec<Result<Vec<u8>, std::io::Error>>) -> Vec<BenchmarkEvent> {
    let mut seen = Vec::new();
    read_json(stream::iter(reads), |event| seen.push(event))
        .await
        .unwrap();
    seen
}

#[tokio::test]
async fn a_model_name_split_across_two_reads_reaches_the_event_whole() {
    let name = "caf\u{e9}-\u{6a21}\u{578b}";
    let started = BenchmarkEvent::ModelStarted {
        model_id: 1,
        model_name: name.to_owned(),
        position: 1,
        total: 1,
    };
    let wire = format!(
        ": keep-alive\n\ndata: {}\n\n",
        serde_json::to_string(&started).unwrap()
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
        let seen = events_of(vec![Ok(bytes[..cut].to_vec()), Ok(bytes[cut..].to_vec())]).await;
        assert!(
            matches!(
                seen.as_slice(),
                [BenchmarkEvent::ModelStarted { model_name, .. }] if model_name == name
            ),
            "cut at byte {cut}: {seen:?}"
        );
    }
}

#[tokio::test]
async fn a_payload_that_is_not_the_event_is_skipped_and_the_next_one_read() {
    let wire =
        b"data: {\"type\":\"nobody_knows\"}\n\ndata: {\"type\":\"run_complete\",\"run_id\":7}\n\n";

    let seen = events_of(vec![Ok(wire.to_vec())]).await;

    assert!(
        matches!(seen.as_slice(), [BenchmarkEvent::RunComplete { run_id: 7 }]),
        "{seen:?}"
    );
}

#[tokio::test]
async fn a_read_that_fails_ends_the_stream_with_its_error() {
    let reads: Vec<Result<Vec<u8>, _>> = vec![
        Ok(b"data: {\"type\":\"run_complete\",\"run_id\":7}\n\n".to_vec()),
        Err(std::io::Error::other("the daemon went away")),
    ];
    let mut seen = 0;

    let error = read_json(stream::iter(reads), |_: BenchmarkEvent| seen += 1)
        .await
        .unwrap_err();

    assert_eq!(
        seen, 1,
        "what arrived before the failure was still handed on"
    );
    assert_eq!(
        format!("{error:#}"),
        "reading event stream: the daemon went away"
    );
}
