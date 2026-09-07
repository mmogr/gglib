//! Minimal scripted HTTP/1.1 server for the retry tests.
//!
//! Hand-rolled on a raw [`TcpListener`] rather than reaching for `wiremock`,
//! `axum`, or `hyper`: `check_boundaries.sh` forbids all three anywhere in
//! `gglib-runtime`'s dependency tree, and `cargo tree --depth 1` sees
//! dev-dependencies too — so pulling one in as a test harness would fail CI.
//! Everything here is built from `tokio`, which the crate already has.
//!
//! Every canned response carries `Connection: close`, so the client never
//! reuses a connection and the accept count is exactly the request count.

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use gglib_proxy::models::ErrorResponse;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// How long the server will wait on one client before abandoning it. Keeps a
/// misbehaving test failing fast instead of hanging CI.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);

/// A scripted server. Responses are served in order; the last one repeats for
/// any further requests.
pub(super) struct TestServer {
    /// Base URL, e.g. `http://127.0.0.1:54321`.
    pub(super) base_url: String,
    requests: Arc<AtomicUsize>,
    /// What each client sent, as far as one read went — enough for the
    /// request line and headers, which is what the tests look at.
    heads: Arc<Mutex<Vec<String>>>,
    handle: JoinHandle<()>,
}

impl TestServer {
    /// Bind on an ephemeral loopback port and start serving `script`.
    pub(super) async fn start(script: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral loopback port");
        let base_url = format!(
            "http://{}",
            listener.local_addr().expect("resolve bound address")
        );

        let requests = Arc::new(AtomicUsize::new(0));
        let heads = Arc::new(Mutex::new(Vec::new()));
        let handle = tokio::spawn(serve(
            listener,
            script,
            Arc::clone(&requests),
            Arc::clone(&heads),
        ));

        Self {
            base_url,
            requests,
            heads,
            handle,
        }
    }

    /// How many requests have been accepted so far.
    pub(super) fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    /// The request heads seen so far, in order.
    pub(super) fn request_heads(&self) -> Vec<String> {
        self.heads.lock().expect("heads mutex").clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Accept loop: one response per connection, then close.
async fn serve(
    listener: TcpListener,
    script: Vec<String>,
    requests: Arc<AtomicUsize>,
    heads: Arc<Mutex<Vec<String>>>,
) {
    loop {
        let Ok((mut socket, _)) = listener.accept().await else {
            return;
        };
        let index = requests.fetch_add(1, Ordering::SeqCst);
        let response = script
            .get(index)
            .or_else(|| script.last())
            .cloned()
            .unwrap_or_default();

        // Drain what the client sent. The content is irrelevant — reading it
        // just stops the peer seeing a reset before it has finished writing.
        let mut scratch = vec![0_u8; 8192];
        if let Ok(Ok(n)) = tokio::time::timeout(CLIENT_TIMEOUT, socket.read(&mut scratch)).await {
            heads
                .lock()
                .expect("heads mutex")
                .push(String::from_utf8_lossy(&scratch[..n]).into_owned());
        }

        let _ = tokio::time::timeout(CLIENT_TIMEOUT, socket.write_all(response.as_bytes())).await;
        let _ = socket.shutdown().await;
    }
}

/// A JSON response with the given status.
pub(super) fn json(status: u16, reason: &str, body: &str) -> String {
    with_headers(status, reason, body, "application/json", &[])
}

/// A JSON response carrying extra headers, e.g. `Retry-After`.
pub(super) fn json_with(status: u16, reason: &str, body: &str, extra: &[(&str, &str)]) -> String {
    with_headers(status, reason, body, "application/json", extra)
}

/// An SSE response body, as llama-server would stream it.
pub(super) fn sse(frames: &[&str]) -> String {
    let body: String = frames.iter().map(|f| format!("data: {f}\n\n")).collect();
    with_headers(200, "OK", &body, "text/event-stream", &[])
}

fn with_headers(
    status: u16,
    reason: &str,
    body: &str,
    content_type: &str,
    extra: &[(&str, &str)],
) -> String {
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n",
        body.len()
    );
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    head.push_str(body);
    head
}

// ─── The bodies the two real upstreams write ────────────────────────────────
//
// Each has exactly one home. The modelpipe literals are copied verbatim from
// version 0.2.0's `refusal.rs`, the version `gglib-app-services` pins: the
// interesting failures in this area were never about the logic, they were
// about a wire shape that did not look the way the struct said it did, and a
// paraphrase would have gone green against the bug. They live here rather
// than beside one test because two files now read them, and a modelpipe bump
// must have a single place to update.

/// The proxy's error body for an admission timeout — the real wire shape.
pub(super) fn admission_timeout_body() -> String {
    r#"{"error":{"message":"waited without reaching the front of the queue","type":"service_unavailable","code":"admission_timeout"}}"#
        .to_owned()
}

/// This proxy's own 401, from `gglib_proxy::access`'s bearer guard.
///
/// The reason a refused key may not be explained by its `code` alone: this
/// carries the same `invalid_api_key` a far machine's refusal does, on a path
/// where there is no far machine and nothing to re-pair. It is not otherwise
/// the same body — ours names a `type` and says something different — so only
/// the `code` collides, which is exactly what makes the code insufficient.
///
/// Built through [`ErrorResponse`] rather than typed out, because unlike
/// modelpipe's this shape is ours: a change to the struct's serialization
/// reaches this body instead of silently leaving it behind. The three
/// arguments are `access::bearer_guard`'s own.
pub(super) fn proxy_invalid_key_body() -> String {
    serde_json::to_string(&ErrorResponse::with_code(
        "Missing or invalid API key. Send it as 'Authorization: Bearer <key>'.",
        "invalid_request_error",
        "invalid_api_key",
    ))
    .expect("an ErrorResponse always serializes")
}

/// modelpipe's edge 401, written before the backend is contacted at all.
pub(super) fn edge_invalid_api_key_body() -> String {
    r#"{"error":{"message":"invalid or missing bearer token","code":"invalid_api_key"}}"#.to_owned()
}

/// The connect side has no tunnel: the peer is away, or `keep_connected` is
/// dialling a replacement after the laptop changed networks.
pub(super) fn edge_tunnel_unavailable_body() -> String {
    r#"{"error":{"message":"no tunnel to the serving side is connected right now","code":"tunnel_unavailable"}}"#
        .to_owned()
}

/// The serving side reached for its model server and found nothing there.
pub(super) fn edge_backend_unreachable_body() -> String {
    r#"{"error":{"message":"the serving side could not reach its backend","code":"backend_unreachable"}}"#
        .to_owned()
}
