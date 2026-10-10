//! A run's event stream as the CLI reads it.

use std::ops::ControlFlow;

use futures_util::stream;
use gglib_core::domain::runs::{RunInfo, RunKind, RunStatus};
use gglib_core::sse::DataFrames;

use super::{RunItem, drain_items, read_events};

fn ended() -> RunInfo {
    RunInfo {
        id: "r1".into(),
        kind: RunKind::Chat,
        status: RunStatus::Completed,
        model: None,
        device: None,
        created_at_ms: 1,
        finished_at_ms: Some(2),
        conversation_id: None,
        last_seq: 2,
        error: None,
        frames: gglib_core::domain::runs::RunFrames::Openai,
    }
}

fn wire() -> String {
    format!(
        ": keep-alive\n\nid: 1\ndata: {{\"a\":1}}\n\nid: 2\ndata: caf\u{e9}\n\nevent: run\ndata: {}\n\n",
        serde_json::to_string(&ended()).unwrap()
    )
}

#[test]
fn frames_carry_their_number_and_the_run_event_its_final_state() {
    let mut frames = DataFrames::unbounded();
    let items = drain_items(&mut frames, wire().as_bytes()).unwrap();
    assert_eq!(
        items,
        [
            RunItem::Frame {
                seq: 1,
                data: "{\"a\":1}".into()
            },
            RunItem::Frame {
                seq: 2,
                data: "caf\u{e9}".into()
            },
            RunItem::End(ended()),
        ]
    );
}

/// Split one byte at a time, so the `é` arrives in two halves.
#[tokio::test]
async fn a_stream_in_pieces_reads_whole_and_returns_the_end() {
    let bytes = wire().into_bytes();
    let chunks: Vec<Result<Vec<u8>, std::io::Error>> = bytes.iter().map(|b| Ok(vec![*b])).collect();
    let mut seen = Vec::new();

    let end = read_events(stream::iter(chunks), |item| {
        seen.push(format!("{item:?}"));
        ControlFlow::Continue(())
    })
    .await
    .unwrap();

    assert_eq!(end, Some(ended()));
    assert_eq!(seen.len(), 3);
    assert!(seen[1].contains("caf\u{e9}"), "{seen:?}");
}

#[tokio::test]
async fn a_break_stops_reading_with_no_end() {
    let chunks = vec![Ok::<_, std::io::Error>(wire().into_bytes())];
    let mut frames = 0;

    let end = read_events(stream::iter(chunks), |_| {
        frames += 1;
        ControlFlow::Break(())
    })
    .await
    .unwrap();

    assert_eq!((end, frames), (None, 1));
}

#[test]
fn an_event_with_data_and_no_number_is_refused() {
    let mut frames = DataFrames::unbounded();
    let error = drain_items(&mut frames, b"id: one\ndata: {}\n\n").unwrap_err();
    assert_eq!(error.to_string(), "an event without its number");

    let error = drain_items(&mut frames, b"data: {}\n\n").unwrap_err();
    assert_eq!(error.to_string(), "an event without its number");
}
