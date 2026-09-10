//! `prompt_progress` is the proxy's data, not the client's frame.
//!
//! The proxy forces `return_progress: true` upstream so the dashboard has a
//! pre-fill bar. A `prompt_progress` chunk carries no `choices` key, which is
//! a llama.cpp extension and not `OpenAI` streaming JSON, so re-emitting it to
//! a client that never asked kills every schema-validating client (anything on
//! the Vercel AI SDK — `OpenCode` among them) on the first frame, before a
//! single token arrives.

use super::*;
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

/// Drain one turn through the streaming path and return every `data:` frame
/// the client saw, parsed.
async fn progress_turn_frames(client_wants_progress: bool) -> Vec<serde_json::Value> {
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

    wire.lines()
        .filter_map(|l| l.strip_prefix("data: "))
        .filter(|d| *d != "[DONE]")
        .map(|d| serde_json::from_str(d).expect("every forwarded frame is JSON"))
        .collect()
}

#[tokio::test]
async fn a_client_that_did_not_ask_never_sees_a_frame_without_choices() {
    let frames = progress_turn_frames(false).await;

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
        "dropping progress frames must not drop the turn's text: {frames:?}"
    );
}

#[tokio::test]
async fn a_client_that_asked_for_progress_still_gets_it() {
    let frames = progress_turn_frames(true).await;

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
