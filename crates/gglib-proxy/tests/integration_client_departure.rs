//! A client that leaves a streamed chat completion before llama-server has
//! generated anything, through a real `serve`: the proxy drops its request to
//! the upstream at once, and the stream ends, freeing the model, with no
//! KV-cache save sent to the silent server and no empty response counted
//! against the model.
//!
//! Both stream bounds are longer than any of these tests waits, so no bound
//! can end a request here. Before the reply's headers the proxy also sends a
//! keepalive every 15 s, whose send fails once the client has gone; these
//! tests ask for the upstream to be freed well before the first one.

mod fixtures;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::{Router, body::Body, http::Response, routing::post};
use bytes::Bytes;
use futures_util::StreamExt as _;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use fixtures::common::FixedUpstream;
use fixtures::stall::{MODEL, spawn_proxy_under};
use gglib_proxy::StreamBounds;

/// Bounds none of these tests waits out.
const BOUNDS: StreamBounds = StreamBounds {
    first_byte: Duration::from_secs(60),
    idle: Duration::from_secs(60),
};

/// How soon after the client leaves the upstream must be freed: well under
/// the first keepalive.
const AT_ONCE: Duration = Duration::from_secs(2);

/// Longer than the first keepalive, so a request freed by it fails on its
/// timing rather than on a timeout.
const PATIENCE: Duration = Duration::from_secs(20);

/// What the stand-in upstream does with a chat request.
#[derive(Debug, Clone, Copy)]
enum Upstream {
    /// Sends its headers and one prefill progress frame, then nothing.
    SilentPrefill,
    /// Never sends its headers: no slot for the request.
    NoHeaders,
}

/// Each chat request the stand-in receives hands the test a receiver that
/// resolves once the upstream no longer holds that request: when the proxy
/// drops it.
type Arrivals = mpsc::UnboundedReceiver<oneshot::Receiver<()>>;

async fn chat(
    kind: Upstream,
    arrivals: mpsc::UnboundedSender<oneshot::Receiver<()>>,
) -> Response<Body> {
    let (held, released) = oneshot::channel::<()>();
    arrivals.send(released).expect("the test is listening");
    match kind {
        Upstream::NoHeaders => {
            let _held = held;
            std::future::pending().await
        }
        Upstream::SilentPrefill => {
            let body = async_stream::stream! {
                let _held = held;
                yield Ok::<_, std::io::Error>(Bytes::from_static(
                    b"data: {\"prompt_progress\":{\"cache\":0,\"processed\":10,\"total\":57,\"time_ms\":5}}\n\n",
                ));
                std::future::pending::<()>().await;
            };
            Response::builder()
                .header("content-type", "text/event-stream")
                .body(Body::from_stream(body))
                .expect("a valid response")
        }
    }
}

/// The stand-in llama-server: `kind` for chat requests, and KV-cache saves
/// that it counts and never answers.
struct Stand {
    /// The proxy's base URL.
    base: String,
    arrivals: Arrivals,
    saves: Arc<AtomicU64>,
}

/// Start the stand-in upstream and a real `serve` in front of it, with the KV
/// cache on when there is a `slot_dir`.
async fn spawn(kind: Upstream, slot_dir: Option<PathBuf>, cancel: &CancellationToken) -> Stand {
    let (arrived, arrivals) = mpsc::unbounded_channel();
    let saves = Arc::new(AtomicU64::new(0));
    let counted = Arc::clone(&saves);
    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(move || chat(kind, arrived.clone())),
        )
        .route(
            "/slots/0",
            post(move || {
                counted.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<()>()
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
    let base = spawn_proxy_under(BOUNDS, runtime, slot_dir, cancel.clone()).await;
    Stand {
        base,
        arrivals,
        saves,
    }
}

/// Send one streaming chat request and return its response once the proxy's
/// headers arrive, which it sends before the upstream answers.
async fn ask(base: &str, with_progress: bool) -> reqwest::Response {
    let body = json!({
        "model": MODEL,
        "messages": [{"role": "user", "content": "hi"}],
        "stream": true,
        "return_progress": with_progress,
    });
    let send = reqwest::Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&body)
        .send();
    let resp = tokio::time::timeout(PATIENCE, send)
        .await
        .expect("the proxy answers within PATIENCE")
        .expect("the proxy answers");
    assert_eq!(resp.status(), 200);
    resp
}

/// Read the reply's first frame, a prefill progress frame, which shows the
/// proxy is past the headers and waiting on the upstream's next bytes.
async fn past_the_headers(
    resp: reqwest::Response,
) -> impl futures_util::Stream<Item = reqwest::Result<Bytes>> {
    let mut stream = resp.bytes_stream();
    let first = tokio::time::timeout(PATIENCE, stream.next())
        .await
        .expect("the progress frame arrives")
        .expect("the stream is open")
        .expect("the stream reads");
    assert!(
        String::from_utf8_lossy(&first).contains("prompt_progress"),
        "{first:?}"
    );
    stream
}

/// The request the upstream is holding, once the proxy has sent it.
async fn arrival(arrivals: &mut Arrivals) -> oneshot::Receiver<()> {
    tokio::time::timeout(PATIENCE, arrivals.recv())
        .await
        .expect("the proxy forwards the request")
        .expect("the stand-in is running")
}

/// How long after `left` the upstream stopped holding its request.
async fn freed_after(held: oneshot::Receiver<()>, left: Instant) -> Duration {
    let freed = tokio::time::timeout(PATIENCE, held).await;
    assert!(
        freed.is_ok(),
        "the upstream still held the request {PATIENCE:?} after the client left"
    );
    left.elapsed()
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

/// How many requests the dashboard shows in flight.
async fn in_flight(base: &str) -> usize {
    status(base).await["active_connections"]
        .as_array()
        .expect("active_connections is a list")
        .len()
}

/// Wait up to `within` for the dashboard to show nothing in flight.
async fn assert_idle_within(base: &str, within: Duration) {
    let deadline = Instant::now() + within;
    while in_flight(base).await > 0 {
        assert!(
            Instant::now() < deadline,
            "still in flight after {within:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn a_client_that_leaves_during_a_silent_prefill_frees_the_upstream_at_once() {
    let cancel = CancellationToken::new();
    let Stand {
        base, mut arrivals, ..
    } = spawn(Upstream::SilentPrefill, None, &cancel).await;

    let resp = ask(&base, true).await;
    let held = arrival(&mut arrivals).await;
    let stream = past_the_headers(resp).await;
    assert_eq!(in_flight(&base).await, 1);

    let left = Instant::now();
    drop(stream);
    let freed = freed_after(held, left).await;

    assert!(
        freed < AT_ONCE,
        "the upstream was freed {freed:?} after the client left"
    );
    assert_idle_within(&base, AT_ONCE).await;
    // A departure, not a model that answered with nothing.
    let status = status(&base).await;
    let counts = (
        status["per_model_defects"][MODEL]["empty_responses"].as_u64(),
        status["upstream_health"]["total_client_aborts"].as_u64(),
    );
    assert_eq!(counts, (Some(0), Some(1)), "{status}");
    cancel.cancel();
}

#[tokio::test]
async fn a_client_that_leaves_before_the_reply_begins_frees_the_upstream_at_once() {
    let cancel = CancellationToken::new();
    let Stand {
        base, mut arrivals, ..
    } = spawn(Upstream::NoHeaders, None, &cancel).await;

    let resp = ask(&base, false).await;
    // The proxy's request is with the upstream, which has sent no headers.
    let held = arrival(&mut arrivals).await;
    assert_eq!(in_flight(&base).await, 1);

    let left = Instant::now();
    drop(resp);
    let freed = freed_after(held, left).await;

    assert!(
        freed < AT_ONCE,
        "the upstream was freed {freed:?} after the client left"
    );
    assert_idle_within(&base, AT_ONCE).await;
    cancel.cancel();
}

#[tokio::test]
async fn a_client_that_leaves_during_a_silent_prefill_is_not_followed_by_a_kv_cache_save() {
    let cancel = CancellationToken::new();
    let slots = tempfile::tempdir().expect("a temp dir");
    let stand = spawn(
        Upstream::SilentPrefill,
        Some(slots.path().to_owned()),
        &cancel,
    )
    .await;

    let stream = past_the_headers(ask(&stand.base, true).await).await;
    drop(stream);

    // A save sent to this upstream would never be answered, and would keep
    // the request in flight.
    assert_idle_within(&stand.base, AT_ONCE).await;
    assert_eq!(stand.saves.load(Ordering::SeqCst), 0);
    cancel.cancel();
}
