//! `prompt_progress` is the proxy's data, not the client's frame.
//!
//! The proxy forces `return_progress: true` upstream so the dashboard has a
//! pre-fill bar. A `prompt_progress` chunk carries no `choices` key, which is
//! a llama.cpp extension and not `OpenAI` streaming JSON, so re-emitting it to
//! a client that never asked kills every schema-validating client (anything on
//! the Vercel AI SDK — `OpenCode` among them) on the first frame, before a
//! single token arrives. That client is sent an SSE comment for each frame
//! instead, so a long prefill does not go silent on it (#1213).

use super::*;
use gglib_core::sse::SseStreamDecoder;
use serde_json::json;

/// A mock upstream that pre-fills before it speaks — the shape that crashed
/// `OpenCode` through the proxy.
async fn spawn_progress_mock() -> (u16, tokio::task::JoinHandle<()>) {
    use axum::routing::post;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let port = listener.local_addr().unwrap().port();

    let app = axum::Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            let sse = concat!(
                "data: {\"prompt_progress\":{\"cache\":0,\"processed\":0,\"total\":57,\"time_ms\":0}}\n\n",
                "data: {\"prompt_progress\":{\"cache\":0,\"processed\":57,\"total\":57,\"time_ms\":813}}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Moonlight\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n",
            );
            axum::response::Response::builder()
                .header("content-type", "text/event-stream")
                .body(axum::body::Body::from(sse))
                .unwrap()
        }),
    );

    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (port, handle)
}

/// One turn through the streaming path, as the client and the dashboard saw it.
struct ProgressTurn {
    /// Every byte the client was sent, in order.
    wire: String,
    /// The `data:` frames, parsed, `[DONE]` left out.
    frames: Vec<serde_json::Value>,
    /// The connection's dashboard entry once the turn was drained.
    dashboard: crate::connections::ActiveConnectionSnapshot,
}

/// Drain one turn from [`spawn_progress_mock`] through the streaming path.
async fn progress_turn(client_wants_progress: bool) -> ProgressTurn {
    let (port, server) = spawn_progress_mock().await;
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let resp = Client::new()
        .post(&url)
        .body(Bytes::from_static(b"{}"))
        .send()
        .await
        .expect("mock upstream reachable");

    let registry = Arc::new(crate::connections::ActiveConnectionsRegistry::new());
    let connection = registry.register("m", true, None);
    let (tx, mut rx) = tokio::sync::mpsc::channel(64);

    stream_response_to_channel(
        resp,
        "m".to_owned(),
        None,
        tx,
        &connection,
        None,
        client_wants_progress,
    )
    .await;

    let mut wire = String::new();
    while let Ok(Some(Ok(chunk))) =
        tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await
    {
        wire.push_str(&String::from_utf8_lossy(&chunk));
    }
    server.abort();

    let frames = wire
        .lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter(|d| *d != "[DONE]")
        .map(|d| serde_json::from_str(d).expect("every forwarded frame is JSON"))
        .collect();
    let [dashboard] = registry.snapshot().try_into().expect("one connection");
    ProgressTurn {
        wire,
        frames,
        dashboard,
    }
}

/// The SSE comment lines in `wire`, in order.
fn comments(wire: &str) -> Vec<&str> {
    wire.lines().filter(|l| l.starts_with(':')).collect()
}

#[tokio::test]
async fn a_client_that_did_not_ask_never_sees_a_frame_without_choices() {
    let frames = progress_turn(false).await.frames;

    assert!(
        frames.iter().all(|f| f.get("prompt_progress").is_none()),
        "prompt_progress must not reach a client that never asked: {frames:?}"
    );
    assert!(
        frames.iter().all(|f| f.get("choices").is_some()),
        "every forwarded chunk must carry choices — the one thing a strict \
         OpenAI client validates: {frames:?}"
    );
    assert!(
        frames
            .iter()
            .any(|f| f["choices"][0]["delta"]["content"] == json!("Moonlight")),
        "the prefill comments must not cost the turn its text: {frames:?}"
    );
}

#[tokio::test]
async fn a_client_that_did_not_ask_gets_a_comment_for_each_progress_frame_before_the_text() {
    let turn = progress_turn(false).await;

    assert_eq!(
        comments(&turn.wire),
        [": prefill 0/57", ": prefill 57/57"],
        "one comment per frame, with its numbers, in order: {}",
        turn.wire
    );
    assert!(
        turn.wire
            .starts_with(": prefill 0/57\n\n: prefill 57/57\n\ndata: "),
        "each comment is a whole SSE block, ahead of the first frame: {}",
        turn.wire
    );
}

#[tokio::test]
async fn the_prefill_comments_are_skipped_by_the_sse_decoder() {
    let wire = progress_turn(false).await.wire;

    let (events, ended) = SseStreamDecoder::default().feed_bytes(wire.as_bytes());
    let events: Vec<LlmStreamEvent> = events
        .into_iter()
        .map(|e| e.expect("every event decodes"))
        .collect();

    assert!(ended, "the decoder reached [DONE]: {wire}");
    assert_eq!(
        events,
        [
            LlmStreamEvent::TextDelta {
                content: "Moonlight".to_owned()
            },
            LlmStreamEvent::Done {
                finish_reason: Some("stop".to_owned())
            },
        ],
        "the comments decode to nothing and cost no frame: {wire}"
    );
}

#[tokio::test]
async fn a_client_that_asked_for_progress_still_gets_it() {
    let frames = progress_turn(true).await.frames;

    let progress: Vec<_> = frames
        .iter()
        .filter(|f| f.get("prompt_progress").is_some())
        .collect();
    assert_eq!(
        progress.len(),
        2,
        "both upstream progress frames belong to a client that asked: {frames:?}"
    );
    assert_eq!(progress[1]["prompt_progress"]["processed"], json!(57));
}

#[tokio::test]
async fn a_client_that_asked_for_progress_gets_no_prefill_comment() {
    let wire = progress_turn(true).await.wire;

    assert_eq!(comments(&wire), Vec::<&str>::new(), "{wire}");
}

#[tokio::test]
async fn the_dashboard_records_the_prefill_whether_or_not_the_client_asked() {
    for client_wants_progress in [false, true] {
        let seen = progress_turn(client_wants_progress).await.dashboard;

        assert_eq!(
            (
                seen.prompt_processed,
                seen.prompt_total,
                seen.prompt_time_ms
            ),
            (Some(57), Some(57), Some(813)),
            "client_wants_progress: {client_wants_progress}"
        );
    }
}
