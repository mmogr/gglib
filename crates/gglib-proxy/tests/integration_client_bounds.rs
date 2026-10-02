//! The two waits that once had no end (#1125), through a real `serve`: a
//! streamed reply to a client that stopped reading, and a request that does
//! not stream to an upstream that never finishes. Each ends at its bound and
//! frees the upstream; a request that finishes inside its bound is answered
//! as before.
//!
//! The upstream's own bounds are longer than any of these tests waits, so
//! only the two under test can end a request here.

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

/// Short bounds for a client that stops reading and for a request that does
/// not stream; long ones for the upstream.
const BOUNDS: StreamBounds = StreamBounds {
    first_byte: Duration::from_mins(1),
    idle: Duration::from_mins(1),
    send: Duration::from_millis(300),
    unary: Duration::from_secs(1),
};

/// How long a test waits for what it expects, before it fails.
const PATIENCE: Duration = Duration::from_secs(10);

/// What the stand-in upstream does with each request.
#[derive(Debug, Clone, Copy)]
enum Upstream {
    /// Streams answer text for as long as the proxy reads it.
    Endless,
    /// Never answers.
    Never,
    /// Answers after this long, well inside [`BOUNDS`]'s `unary`.
    After(Duration),
}

/// Each request the stand-in receives hands the test a receiver that resolves
/// once the upstream no longer holds that request: when the proxy drops it.
type Arrivals = mpsc::UnboundedReceiver<oneshot::Receiver<()>>;

async fn answer(
    kind: Upstream,
    arrivals: mpsc::UnboundedSender<oneshot::Receiver<()>>,
) -> Response<Body> {
    let (held, released) = oneshot::channel::<()>();
    arrivals.send(released).expect("the test is listening");
    match kind {
        Upstream::Endless => {
            let delta =
                json!({"choices": [{"index": 0, "delta": {"content": "x".repeat(16 * 1024)}}]});
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
        Upstream::Never => {
            let _held = held;
            std::future::pending().await
        }
        Upstream::After(delay) => {
            tokio::time::sleep(delay).await;
            let answer = json!({"choices": [{"index": 0, "finish_reason": "stop",
                "message": {"role": "assistant", "content": "done"}}]});
            Response::new(Body::from(answer.to_string()))
        }
    }
}

/// Start the stand-in upstream and a real `serve` under [`BOUNDS`] in front of
/// it; returns the proxy's base URL and the upstream's arrivals.
async fn spawn(
    kind: Upstream,
    tags: Vec<String>,
    cancel: &CancellationToken,
) -> (String, Arrivals) {
    let (arrived, arrivals) = mpsc::unbounded_channel();
    let for_embeddings = arrived.clone();
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(move || answer(kind, arrived.clone())),
        )
        .route(
            "/v1/embeddings",
            post(move || answer(kind, for_embeddings.clone())),
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
    let base = spawn_proxy_under(BOUNDS, runtime, tags, None, cancel.clone()).await;
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

/// A chat request for the test model.
fn chat(stream: bool) -> Value {
    json!({"model": MODEL, "messages": [{"role": "user", "content": "hi"}], "stream": stream})
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

/// Assert `response` is the 504 a request that outlasted its bound gets.
async fn assert_upstream_timeout(response: reqwest::Response) {
    assert_eq!(response.status(), 504);
    let body: Value = response.json().await.expect("the error is JSON");
    assert_eq!(body["error"]["code"], "upstream_timeout", "{body}");
    assert_eq!(body["error"]["type"], "server_error", "{body}");
}

#[tokio::test]
async fn a_client_that_stops_reading_a_stream_is_let_go_and_the_upstream_freed() {
    let cancel = CancellationToken::new();
    let (base, mut arrivals) = spawn(Upstream::Endless, vec![], &cancel).await;

    // The headers arrive, and then the client reads nothing, while the
    // upstream writes until every buffer between them is full.
    let unread = post_json(&base, "/v1/chat/completions", &chat(true)).await;
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

#[tokio::test]
async fn a_chat_completion_that_does_not_stream_ends_at_its_bound_with_upstream_timeout() {
    let cancel = CancellationToken::new();
    let (base, mut arrivals) = spawn(Upstream::Never, vec![], &cancel).await;

    let started = Instant::now();
    let response = post_json(&base, "/v1/chat/completions", &chat(false)).await;

    assert!(started.elapsed() >= BOUNDS.unary, "{:?}", started.elapsed());
    assert_upstream_timeout(response).await;
    assert_upstream_freed(&mut arrivals).await;
    assert_nothing_in_flight(&base).await;
    cancel.cancel();
}

#[tokio::test]
async fn a_chat_completion_that_finishes_inside_its_bound_is_answered() {
    let cancel = CancellationToken::new();
    let (base, _arrivals) = spawn(Upstream::After(BOUNDS.unary / 3), vec![], &cancel).await;

    let response = post_json(&base, "/v1/chat/completions", &chat(false)).await;

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.expect("the answer is JSON");
    assert_eq!(body["choices"][0]["message"]["content"], "done", "{body}");
    cancel.cancel();
}

#[tokio::test]
async fn an_embeddings_request_ends_at_the_same_bound_with_upstream_timeout() {
    let cancel = CancellationToken::new();
    let (base, mut arrivals) = spawn(Upstream::Never, vec!["embedding".into()], &cancel).await;

    let started = Instant::now();
    let embed = json!({"model": MODEL, "input": "hello"});
    let response = post_json(&base, "/v1/embeddings", &embed).await;

    assert!(started.elapsed() >= BOUNDS.unary, "{:?}", started.elapsed());
    assert_upstream_timeout(response).await;
    assert_upstream_freed(&mut arrivals).await;
    assert_nothing_in_flight(&base).await;
    cancel.cancel();
}
