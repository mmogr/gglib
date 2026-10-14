//! End to end: what a turn generated is read back off `/v1/proxy/status`,
//! on the turn's own entry in `recent_requests`.
//!
//! The unit tests prove the drain's count and the store's write. These prove
//! the chain between them, for a streamed turn and for one that is not, and
//! what an entry holds before its turn is over and after its client hung up.

use std::sync::Arc;
use std::time::Duration;

use axum::{Router, body::Body, http::Response, routing::post};
use bytes::Bytes;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use gglib_proxy::StreamBounds;

mod fixtures;
use fixtures::common::{FixedUpstream, spawn_mock_upstream, spawn_proxy};
use fixtures::stall::{MODEL, spawn_proxy_under};

/// A streamed turn: seven characters of reasoning in two frames, a call in
/// two fragments, a second call, a finish reason, and a usage frame.
const STREAMED_TURN: &[u8] = b"\
data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"two \"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"cal\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\":\\\"Paris\\\"}\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_b\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{\\\"city\\\":\\\"Oslo\\\"}\"}}]},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":37,\"total_tokens\":48}}\n\n\
data: [DONE]\n\n";

/// A whole answer: five characters of reasoning, one call, and its usage.
const UNARY_ANSWER: &[u8] = b"{\"choices\":[{\"index\":0,\"finish_reason\":\"tool_calls\",\"message\":{\
\"role\":\"assistant\",\"reasoning_content\":\"think\",\"tool_calls\":[{\"id\":\"call_a\",\"type\":\"function\",\
\"function\":{\"name\":\"get_weather\",\"arguments\":\"{}\"}}]}}],\
\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":23,\"total_tokens\":34}}";

/// Send one turn to a fresh proxy whose upstream answers with `answer`, read
/// its reply to the end, and return its entry in `recent_requests` once
/// `written` accepts it.
async fn entry_after_a_turn(
    answer: &'static [u8],
    streaming: bool,
    written: impl Fn(&Value) -> bool,
) -> Value {
    let upstream_cancel = CancellationToken::new();
    let upstream_port = spawn_mock_upstream(vec![answer], upstream_cancel.clone()).await;
    let (proxy_url, proxy_cancel) = spawn_proxy(upstream_port, "test-model", vec![]).await;

    let client = Client::new();
    let reply = client
        .post(format!("{proxy_url}/v1/chat/completions"))
        .json(&json!({
            "model": "test-model",
            "stream": streaming,
            "messages": [{"role": "user", "content": "weather?"}],
        }))
        .send()
        .await
        .expect("proxy request");
    assert_eq!(reply.status(), 200);
    let mut body = reply.bytes_stream();
    while let Some(chunk) = body.next().await {
        chunk.expect("body chunk");
    }

    // A streamed turn's readings are written by its task once the body has
    // ended, hence the bounded poll.
    let mut entry = Value::Null;
    for _ in 0..50 {
        let status: Value = client
            .get(format!("{proxy_url}/v1/proxy/status"))
            .send()
            .await
            .expect("status request")
            .json()
            .await
            .expect("status json");
        let entries = status["recent_requests"].as_array().expect("an array");
        assert_eq!(entries.len(), 1, "one turn, one entry: {status}");
        entry = entries[0].clone();
        if written(&entry) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    proxy_cancel.cancel();
    upstream_cancel.cancel();
    entry
}

#[tokio::test]
async fn a_streamed_turns_generation_is_on_its_status_entry() {
    let entry = entry_after_a_turn(STREAMED_TURN, true, |e| !e["finish_reason"].is_null()).await;

    assert_eq!(entry["completion_tokens"], 37, "{entry}");
    assert_eq!(entry["reasoning_chars"], 7, "{entry}");
    assert_eq!(entry["tool_calls"], 2, "{entry}");
    assert_eq!(entry["finish_reason"], "tool_calls", "{entry}");
    assert!(
        entry["generation_ms"].is_u64(),
        "five deltas were timed: {entry}"
    );
}

/// Bounds no test here waits out, so none of them can end a turn.
const NO_BOUND: StreamBounds = StreamBounds {
    first_byte: Duration::from_mins(1),
    idle: Duration::from_mins(1),
    send: Duration::from_mins(1),
    unary: Duration::from_mins(1),
};

/// Characters of reasoning in every reasoning frame of an endless turn. Each
/// frame's reasoning is eight bytes long, so a count of bytes is no multiple
/// of this.
const FRAME_CHARS: u64 = 7;

/// How an endless turn opens: [`FRAME_CHARS`] of reasoning, then one call.
const OPENING: &[u8] = b"\
data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"think\xc3\xa9!\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_a\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"{}\"}}]},\"finish_reason\":null}]}\n\n";

/// How it goes on: [`FRAME_CHARS`] more of reasoning.
const MORE: &[u8] = b"data: {\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"tick-\xc3\xa9.\"},\"finish_reason\":null}]}\n\n";

/// A proxy over an upstream whose turn never ends.
struct EndlessTurn {
    /// The proxy's base URL.
    base: String,
    /// Told once the client has read the opening, which lets the rest go on.
    opening_read: Arc<Notify>,
}

/// A proxy over an upstream whose turn never ends: [`OPENING`], then, once
/// the client has read it, [`MORE`] every 20 ms for as long as the proxy
/// holds the request. Every later frame is therefore generated at least
/// 20 ms after the first. There is no finish, no usage frame and no bound to
/// meet, so the proxy stops reading the turn only when a frame it sends
/// finds the client gone.
async fn spawn_proxy_over_an_endless_turn(cancel: &CancellationToken) -> EndlessTurn {
    let opening_read = Arc::new(Notify::new());
    let go_on = Arc::clone(&opening_read);
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || {
            let go_on = Arc::clone(&go_on);
            async move {
                let body = async_stream::stream! {
                    yield Ok::<_, std::io::Error>(Bytes::from_static(OPENING));
                    go_on.notified().await;
                    loop {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        yield Ok(Bytes::from_static(MORE));
                    }
                };
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .body(Body::from_stream(body))
                    .expect("a valid response")
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("bound").port();
    let shutdown = cancel.clone().cancelled_owned();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
            .ok();
    });
    let runtime = Arc::new(FixedUpstream {
        port,
        model_name: MODEL.into(),
        slot_restore_supported: true,
        pinned: false,
    });
    let base = spawn_proxy_under(NO_BOUND, runtime, vec![], None, cancel.clone()).await;
    EndlessTurn { base, opening_read }
}

/// Ask for `turn`, streamed, and read its reply as far as the reasoning
/// [`OPENING`] begins with, which shows the proxy is reading the upstream's
/// answer, and then until `more` of the [`MORE`] frames have come through as
/// well. A frame the client has read is one the proxy counted. Returns the
/// reply, still open.
async fn reply_past_its_reasoning(
    turn: &EndlessTurn,
    more: usize,
) -> impl futures_util::Stream<Item = reqwest::Result<Bytes>> {
    let base = &turn.base;
    let reply = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": MODEL,
            "stream": true,
            "messages": [{"role": "user", "content": "weather?"}],
        }))
        .send()
        .await
        .expect("proxy request");
    assert_eq!(reply.status(), 200);
    let mut body = reply.bytes_stream();
    // Both markers are ASCII, so a chunk that ends inside a longer character
    // cannot hide one.
    let mut read = String::new();
    let mut opened = false;
    while !opened || read.matches("tick-").count() < more {
        let chunk = tokio::time::timeout(Duration::from_secs(20), body.next())
            .await
            .expect("the reasoning arrives")
            .expect("the reply is open")
            .expect("the reply reads");
        read.push_str(&String::from_utf8_lossy(&chunk));
        if !opened && read.contains("think") {
            opened = true;
            turn.opening_read.notify_one();
        }
    }
    body
}

/// The one entry in `base`'s `recent_requests`.
async fn the_entry(base: &str) -> Value {
    let status: Value = reqwest::get(format!("{base}/v1/proxy/status"))
        .await
        .expect("status request")
        .json()
        .await
        .expect("status json");
    let entries = status["recent_requests"].as_array().expect("an array");
    assert_eq!(entries.len(), 1, "one turn, one entry: {status}");
    entries[0].clone()
}

/// The readings are written when the turn is over. Until then the entry
/// holds the defaults it was recorded with: no readings, which is not
/// readings of zero.
#[tokio::test]
async fn an_entry_read_while_its_turn_streams_holds_no_readings() {
    let cancel = CancellationToken::new();
    let turn = spawn_proxy_over_an_endless_turn(&cancel).await;
    let _reply = reply_past_its_reasoning(&turn, 0).await;

    let entry = the_entry(&turn.base).await;

    assert!(entry["completion_tokens"].is_null(), "{entry}");
    assert_eq!(entry["reasoning_chars"], 0, "{entry}");
    assert_eq!(entry["tool_calls"], 0, "{entry}");
    assert!(entry["finish_reason"].is_null(), "{entry}");
    assert!(entry["generation_ms"].is_null(), "{entry}");
    cancel.cancel();
}

/// A turn the client hung up on still has its readings written: those taken
/// before the proxy found it gone. The call was held for repair and never
/// queued for the client, so it is not counted.
///
/// The client reads the opening and three more frames before it goes, so the
/// drain counted four frames of reasoning at least, and timed a token 20 ms
/// or more after the first.
#[tokio::test]
async fn a_turn_the_client_hung_up_on_keeps_its_partial_readings() {
    let cancel = CancellationToken::new();
    let turn = spawn_proxy_over_an_endless_turn(&cancel).await;
    let base = turn.base.clone();
    let reply = reply_past_its_reasoning(&turn, 3).await;

    drop(reply);

    // The readings are written once a send has found the client gone, hence
    // the bounded poll. Nothing else ends this turn.
    let mut entry = Value::Null;
    for _ in 0..200 {
        entry = the_entry(&base).await;
        if entry["reasoning_chars"] != 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    cancel.cancel();

    let reasoning_chars = entry["reasoning_chars"].as_u64().expect("a count");
    assert_eq!(
        reasoning_chars % FRAME_CHARS,
        0,
        "whole frames of reasoning, counted in characters: {entry}"
    );
    assert!(
        reasoning_chars >= 4 * FRAME_CHARS,
        "the opening and the three frames the client read: {entry}"
    );
    assert_eq!(entry["tool_calls"], 0, "held, never queued: {entry}");
    assert!(
        entry["finish_reason"].is_null(),
        "no finish was read: {entry}"
    );
    assert!(entry["completion_tokens"].is_null(), "nor a usage: {entry}");
    let generation_ms = entry["generation_ms"].as_u64().expect("a time");
    assert!(
        generation_ms > 0,
        "a token was timed after the first: {entry}"
    );
}

#[tokio::test]
async fn a_unary_turns_generation_is_on_its_status_entry() {
    let entry = entry_after_a_turn(UNARY_ANSWER, false, |_| true).await;

    assert_eq!(entry["completion_tokens"], 23, "{entry}");
    assert_eq!(entry["reasoning_chars"], 5, "{entry}");
    assert_eq!(entry["tool_calls"], 1, "{entry}");
    assert_eq!(entry["finish_reason"], "tool_calls", "{entry}");
    assert!(
        entry["generation_ms"].is_null(),
        "no deltas, so no time: {entry}"
    );
}
