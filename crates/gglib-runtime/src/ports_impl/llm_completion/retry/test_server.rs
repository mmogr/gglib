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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

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
        let handle = tokio::spawn(serve(listener, script, Arc::clone(&requests)));

        Self {
            base_url,
            requests,
            handle,
        }
    }

    /// How many requests have been accepted so far.
    pub(super) fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Accept loop: one response per connection, then close.
async fn serve(listener: TcpListener, script: Vec<String>, requests: Arc<AtomicUsize>) {
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
        let _ = tokio::time::timeout(CLIENT_TIMEOUT, socket.read(&mut scratch)).await;

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

/// The proxy's error body for an admission timeout — the real wire shape.
pub(super) fn admission_timeout_body() -> String {
    r#"{"error":{"message":"waited without reaching the front of the queue","type":"service_unavailable","code":"admission_timeout"}}"#
        .to_owned()
}
