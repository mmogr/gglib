//! The wire form of a run's events.

use std::time::Duration;

use axum::response::IntoResponse as _;
use futures_util::StreamExt as _;
use futures_util::stream;
use gglib_core::domain::runs::{RunInfo, RunKind, RunStatus};
use gglib_core::ports::{RunEvent, RunEvents};
use http_body_util::BodyExt as _;
use tokio_util::sync::CancellationToken;

use super::stream;

fn ended() -> RunInfo {
    RunInfo {
        id: "r1".into(),
        kind: RunKind::Chat,
        status: RunStatus::Completed,
        model: None,
        device: None,
        created_at_ms: 1,
        finished_at_ms: Some(2),
        last_seq: 2,
        error: None,
    }
}

async fn body_of(events: RunEvents, shutdown: Option<CancellationToken>) -> String {
    let response = stream(events, shutdown).into_response();
    let bytes = tokio::time::timeout(Duration::from_secs(5), response.into_body().collect())
        .await
        .expect("the stream closes")
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

#[tokio::test]
async fn each_event_carries_its_seq_and_the_end_carries_the_run() {
    let events: RunEvents = Box::pin(stream::iter([
        RunEvent::Frame {
            seq: 1,
            data: "{\"a\":1}".into(),
        },
        RunEvent::Frame {
            seq: 2,
            data: "{\"b\":2}".into(),
        },
        RunEvent::End(ended()),
    ]));

    let body = body_of(events, None).await;

    let info = serde_json::to_string(&ended()).unwrap();
    assert_eq!(
        body,
        format!(
            "id: 1\ndata: {{\"a\":1}}\n\nid: 2\ndata: {{\"b\":2}}\n\nevent: run\ndata: {info}\n\n"
        )
    );
}

#[tokio::test]
async fn the_stream_closes_when_the_daemon_stops() {
    let shutdown = CancellationToken::new();
    let events: RunEvents = Box::pin(
        stream::iter([RunEvent::Frame {
            seq: 1,
            data: "x".into(),
        }])
        .chain(stream::pending()),
    );
    let stopping = shutdown.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        stopping.cancel();
    });

    let body = body_of(events, Some(shutdown)).await;

    assert_eq!(body, "id: 1\ndata: x\n\n");
}
