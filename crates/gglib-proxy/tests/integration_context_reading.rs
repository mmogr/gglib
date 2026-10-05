//! A streamed reply tells the client that asked how large the answering
//! server's context was and how many messages were shortened to fit, and
//! sends every other client the usage frame it always did.
//!
//! The real proxy, in front of a llama-server that streams a short reply and
//! its usage. The fixtures launch every model at a context of 4096.

use std::sync::{Arc, Mutex};

use axum::{Router, body::Body, http::Response, routing::post};
use futures_util::StreamExt as _;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod fixtures;
use fixtures::common::{parse_sse_frames, spawn_proxy};

const MODEL: &str = "reading-model";

/// A reply of one word, its end, and the usage llama-server reports for it.
const REPLY: &str = concat!(
    r#"data: {"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#,
    "\n\n",
    r#"data: {"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
    "\n\n",
    r#"data: {"choices":[],"usage":{"prompt_tokens":812,"completion_tokens":96,"total_tokens":908}}"#,
    "\n\n",
    "data: [DONE]\n\n",
);

/// One streamed turn through the proxy: every byte the client was sent, and
/// the body the proxy forwarded upstream.
async fn streamed(request: Value) -> (String, Value) {
    let forwarded: Arc<Mutex<Option<Value>>> = Arc::default();
    let seen = Arc::clone(&forwarded);
    let upstream = Router::new().route(
        "/v1/chat/completions",
        post(move |body: axum::body::Bytes| {
            let seen = Arc::clone(&seen);
            async move {
                *seen.lock().unwrap() = serde_json::from_slice(&body).ok();
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(Body::from(REPLY))
                    .unwrap()
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();
    let upstream_cancel = CancellationToken::new();
    let stop = upstream_cancel.clone();
    tokio::spawn(async move {
        axum::serve(listener, upstream)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .ok();
    });
    let (proxy_url, proxy_cancel) = spawn_proxy(port, MODEL, Vec::new()).await;

    let response = reqwest::Client::new()
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&request)
        .send()
        .await
        .expect("proxy request");
    assert_eq!(response.status(), 200);
    let mut wire = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        wire.extend_from_slice(&chunk.expect("body chunk"));
    }
    proxy_cancel.cancel();
    upstream_cancel.cancel();

    let forwarded = forwarded.lock().unwrap().take().expect("a forwarded body");
    (String::from_utf8(wire).expect("utf-8"), forwarded)
}

/// A streaming request for `messages`, which asks for progress or does not.
fn request(messages: &Value, return_progress: Option<bool>) -> Value {
    let mut body = json!({ "model": MODEL, "stream": true, "messages": messages });
    if let Some(asked) = return_progress {
        body["return_progress"] = json!(asked);
    }
    body
}

fn hi() -> Value {
    json!([{ "role": "user", "content": "hi" }])
}

/// The one usage frame of `wire`: its text as sent, and parsed.
fn usage_frame(wire: &str) -> (&str, Value) {
    let (frames, done) = parse_sse_frames(wire);
    assert!(done, "the stream ends in [DONE]");
    let usage: Vec<&Value> = frames.iter().filter(|f| f.get("usage").is_some()).collect();
    assert_eq!(usage.len(), 1, "one usage frame in {wire}");
    let text = wire
        .split("\n\n")
        .find(|frame| frame.contains("\"usage\""))
        .expect("the usage frame's text");
    (text, usage[0].clone())
}

/// The reading is the context the answering server was launched with, and
/// it goes only to a client whose own body set `return_progress`. A client
/// that left the key out, or set it false, is sent the usage frame with the
/// upstream's three counts and nothing else, byte for byte.
#[tokio::test]
async fn a_streamed_reply_reports_the_launched_context_only_when_asked() {
    let (wire, _) = streamed(request(&hi(), Some(true))).await;
    let (_, frame) = usage_frame(&wire);
    assert_eq!(
        frame["usage"],
        json!({
            "prompt_tokens": 812,
            "completion_tokens": 96,
            "total_tokens": 908,
            "context_size": 4096,
        }),
        "nothing was shortened, so no count is sent"
    );
    assert_eq!(frame["choices"], json!([]));

    for unasked in [None, Some(false)] {
        let (wire, _) = streamed(request(&hi(), unasked)).await;
        let (text, frame) = usage_frame(&wire);
        let (id, created) = (frame["id"].as_str().unwrap(), &frame["created"]);
        let want = format!(
            "data: {{\"choices\":[],\"created\":{created},\"id\":\"{id}\",\"model\":\"{MODEL}\",\
             \"object\":\"chat.completion.chunk\",\"usage\":{{\"completion_tokens\":96,\
             \"prompt_tokens\":812,\"total_tokens\":908}}}}"
        );
        assert_eq!(text, want, "return_progress: {unasked:?}");
        assert!(!wire.contains("context_size"), "{wire}");
        assert!(!wire.contains("trimmed_messages"), "{wire}");
    }
}

/// A history too long for the context is shortened before it is forwarded,
/// and the client that asked is told how many messages that took: as many
/// as reached the upstream shorter than they were sent.
#[tokio::test]
async fn a_shortened_history_reports_how_many_messages_were_shortened() {
    let long = "x".repeat(9_000);
    let mut messages = Vec::new();
    for _ in 0..4 {
        messages.push(json!({ "role": "user", "content": "and then?" }));
        messages.push(json!({ "role": "assistant", "content": long }));
    }
    // The last eight messages are never shortened; these are short anyway.
    for _ in 0..4 {
        messages.push(json!({ "role": "user", "content": "and then?" }));
        messages.push(json!({ "role": "assistant", "content": "that." }));
    }
    messages.push(json!({ "role": "user", "content": "so?" }));

    let history = json!(messages);

    let (wire, forwarded) = streamed(request(&history, Some(true))).await;

    let shortened = forwarded["messages"]
        .as_array()
        .expect("messages reached the upstream")
        .iter()
        .zip(&messages)
        .filter(|(after, before)| {
            let len = |message: &Value| message["content"].as_str().map_or(0, str::len);
            len(after) < len(before)
        })
        .count();
    assert!(shortened > 0, "a history over the context was shortened");
    let (_, frame) = usage_frame(&wire);
    assert_eq!(frame["usage"]["trimmed_messages"], json!(shortened));
    assert_eq!(frame["usage"]["context_size"], json!(4096));

    // The same history from a client that did not ask: shortened alike, and
    // nothing said of it.
    let (wire, _) = streamed(request(&history, None)).await;
    let (_, frame) = usage_frame(&wire);
    assert_eq!(
        frame["usage"],
        json!({ "prompt_tokens": 812, "completion_tokens": 96, "total_tokens": 908 })
    );
}
