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
        conversation_id: None,
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

/// A tool call's start, the run's preview frame, the call's end, then the
/// run's end, as the SSE bytes a client reads. The preview is
/// `event: preview` with no `id:`, so a reconnect cursor never counts it.
fn preview_stream() -> Vec<RunEvent> {
    use gglib_core::domain::agent::{AgentEvent, ToolCall, ToolResult};
    let start = AgentEvent::ToolCallStart {
        tool_call: ToolCall {
            id: "call_1".into(),
            name: "builtin:generate_image".into(),
            arguments: serde_json::json!({ "prompt": "a red fox in snow" }),
        },
        display_name: "Generate Image".into(),
        args_summary: None,
    };
    let complete = AgentEvent::ToolCallComplete {
        tool_name: "builtin:generate_image".into(),
        result: ToolResult::text("call_1", "Drew 1 image.", true),
        wait_ms: 0,
        execute_duration_ms: 76_000,
        display_name: "Generate Image".into(),
        duration_display: "76.0s".into(),
    };
    let info = RunInfo {
        kind: RunKind::Agent,
        ..ended()
    };
    vec![
        RunEvent::Frame {
            seq: 1,
            data: serde_json::to_string(&start).unwrap().into(),
        },
        RunEvent::Preview {
            tool_call_id: "call_1".into(),
            data: r#"{"tool_call_id":"call_1","frame":{"mime":"image/png","step":3,"total":20,"b64":"iVBORw0KGgo="}}"#.into(),
        },
        RunEvent::Frame {
            seq: 2,
            data: serde_json::to_string(&complete).unwrap().into(),
        },
        RunEvent::End(info),
    ]
}

/// The checked-in `contracts/runs/preview_stream.txt` is exactly these
/// bytes. Run with `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a
/// deliberate change.
#[tokio::test]
async fn a_preview_is_an_event_with_no_id_between_the_frames() {
    let body = body_of(Box::pin(stream::iter(preview_stream())), None).await;

    assert!(
        body.contains("\n\nevent: preview\ndata: {\"tool_call_id\":\"call_1\",\"frame\":"),
        "{body}"
    );
    let preview_block = body.split("\n\n").nth(1).unwrap();
    assert!(!preview_block.contains("id: "), "{preview_block}");

    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/runs/preview_stream.txt");
    if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
        std::fs::write(&path, &body).expect("write preview_stream.txt");
    }
    let have = std::fs::read_to_string(&path).expect("read contracts/runs/preview_stream.txt");
    assert!(
        have == body,
        "contracts/runs/preview_stream.txt is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{body}"
    );
}
