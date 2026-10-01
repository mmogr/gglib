//! A wait that once had no end (#1125), through a real `serve`: a streamed
//! reply to a client that stopped reading ends at the send bound and frees
//! the upstream.
//!
//! The upstream's own bounds are longer than any of these tests waits, so
//! only the one under test can end a request here.

mod fixtures;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{Router, body::Body, http::Response, routing::post};
use bytes::Bytes;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use fixtures::common::FixedUpstream;
use fixtures::stall::{MODEL, spawn_proxy_under};
use gglib_proxy::StreamBounds;

/// A short bound for a client that stops reading; long ones for the upstream.
const BOUNDS: StreamBounds = StreamBounds {
    first_byte: Duration::from_mins(1),
    idle: Duration::from_mins(1),
    send: Duration::from_millis(300),
};

/// How long a test waits for what it expects, before it fails.
const PATIENCE: Duration = Duration::from_secs(10);

/// Each request the stand-in receives hands the test a receiver that resolves
/// once the upstream no longer holds that request: when the proxy drops it.
type Arrivals = mpsc::UnboundedReceiver<oneshot::Receiver<()>>;

/// The stand-in's answer: answer text, streamed for as long as the proxy
/// reads it.
fn endless(arrivals: &mpsc::UnboundedSender<oneshot::Receiver<()>>) -> Response<Body> {
    let (held, released) = oneshot::channel::<()>();
    arrivals.send(released).expect("the test is listening");
    let delta = json!({"choices": [{"index": 0, "delta": {"content": "x".repeat(16 * 1024)}}]});
    let frame = Bytes::from(format!("data: {delta}\n\n"));
    let body = async_stream::stream! {
        let _held = held;
        loop {
            yield Ok::<_, std::io::Error>(frame.clone());
        }
    };
    Response::builder()
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(body))
        .expect("a valid response")
}

/// Start the stand-in upstream and a real `serve` under [`BOUNDS`] in front of
/// it; returns the proxy's base URL and the upstream's arrivals.
async fn spawn(cancel: &CancellationToken) -> (String, Arrivals) {
    let (arrived, arrivals) = mpsc::unbounded_channel();
    let app = Router::new().route(
        "/v1/chat/completions",
        post(move || std::future::ready(endless(&arrived))),
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
        slot_restore_supported: false,
        pinned: false,
    });
    let base = spawn_proxy_under(BOUNDS, runtime, None, cancel.clone()).await;
    (base, arrivals)
}

/// Post `body` to `path` and return the response once its headers arrive.
async fn post_json(base: &str, path: &str, body: &Value) -> reqwest::Response {
    let send = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .json(body)
        .send();
    tokio::time::timeout(PATIENCE, send)
        .await
        .expect("the proxy answers within PATIENCE")
        .expect("the proxy answers")
}

/// A streamed chat request for the test model.
fn chat() -> Value {
    json!({"model": MODEL, "messages": [{"role": "user", "content": "hi"}], "stream": true})
}

/// Wait for the upstream to receive the request, then for it to stop holding
/// it.
async fn assert_upstream_freed(arrivals: &mut Arrivals) {
    let held = tokio::time::timeout(PATIENCE, arrivals.recv())
        .await
        .expect("the proxy forwards the request")
        .expect("the stand-in is running");
    let freed = tokio::time::timeout(PATIENCE, held).await;
    assert!(freed.is_ok(), "the upstream still held the request");
}

/// The proxy's dashboard.
async fn status(base: &str) -> Value {
    reqwest::get(format!("{base}/v1/proxy/status"))
        .await
        .expect("status answers")
        .json()
        .await
        .expect("status is JSON")
}

/// Wait up to [`PATIENCE`] for the dashboard to show nothing in flight: the
/// request's registration is gone, and with it the admission lease it held.
async fn assert_nothing_in_flight(base: &str) {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let connections = status(base).await["active_connections"].clone();
        let in_flight = connections.as_array().expect("a list").len();
        if in_flight == 0 {
            return;
        }
        assert!(Instant::now() < deadline, "still in flight: {connections}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_client_that_stops_reading_a_stream_is_let_go_and_the_upstream_freed() {
    let cancel = CancellationToken::new();
    let (base, mut arrivals) = spawn(&cancel).await;

    // The headers arrive, and then the client reads nothing, while the
    // upstream writes until every buffer between them is full.
    let unread = post_json(&base, "/v1/chat/completions", &chat()).await;
    assert_eq!(unread.status(), 200);

    assert_upstream_freed(&mut arrivals).await;
    assert_nothing_in_flight(&base).await;
    let status = status(&base).await;
    assert_eq!(
        status["upstream_health"]["total_stream_stalls"], 0,
        "{status}"
    );
    drop(unread);
    cancel.cancel();
}
