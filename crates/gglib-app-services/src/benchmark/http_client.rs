//! The two HTTP clients the benchmarks stream over, and why they differ.
//!
//! Split out of `benchmark/mod.rs` because the reasoning below is longer than
//! the code it guards, and because getting it wrong is not a small bug: the
//! wrong timeout shape here does not fail the request, it deletes runs out of
//! one arm of a comparison and reports the survivors as a finding.

use std::time::Duration;

use anyhow::Result;

use super::BenchmarkDeps;

/// How long an agentic-eval stream may produce **no bytes at all** before it
/// is abandoned. This is an idle timeout, not a cap on the whole request.
///
/// 90s is far above the worst time-to-first-byte observed on this suite and
/// far below the ten minutes a stalled run used to cost.
const AGENTIC_STREAM_IDLE_TIMEOUT_SECS: u64 = 90;

/// Connect timeout for the agentic eval. llama-server is on loopback, so a
/// connect that takes longer than this is not going to succeed.
const AGENTIC_CONNECT_TIMEOUT_SECS: u64 = 10;

/// Kept below llama.cpp's own keep-alive close so `reqwest` never hands out a
/// socket the server has already dropped. The default is 90s, which is long
/// enough for the pool to serve a half-closed connection to the next task.
const AGENTIC_POOL_IDLE_TIMEOUT_SECS: u64 = 15;

impl BenchmarkDeps {
    /// Construct the `reqwest::Client` used by **compare** mode.
    ///
    /// Carries a 600s total-request deadline. That is tolerable here only
    /// because compare sends one short prompt per model; it would be wrong for
    /// the agentic eval, which is why that has its own client.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest` cannot build the client (extremely rare —
    /// only fails on TLS initialisation errors).
    pub fn build_http_client() -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build benchmark HTTP client: {e}"))
    }

    /// Construct the `reqwest::Client` used by the **agentic eval**.
    ///
    /// # Why this is not [`Self::build_http_client`]
    ///
    /// `ClientBuilder::timeout` is a *total-request* deadline: reqwest applies
    /// it from the start of the connection until the **response body has
    /// finished**, enforced inside `TotalTimeoutBody::poll_frame`. On an SSE
    /// stream it therefore fires mid-body, and `Response::bytes_stream` wraps
    /// every body error — the deadline included — through `error::decode`, so
    /// it surfaces as the uninformative `error decoding response body`.
    ///
    /// The 2026-08-28 eval lost five runs to exactly that: each sat for
    /// 600.00s and then died with a message that named neither the timeout nor
    /// the stall behind it, and the arm they landed in was scored as though the
    /// model had answered wrongly. `crates/gglib-proxy/src/server.rs` avoids
    /// the identical trap and says so in a comment; this client had not learned
    /// it yet.
    ///
    /// An **idle** timeout is the right shape. `build_chat_body` always sets
    /// `return_progress: true`, so llama-server emits `prompt_progress` frames
    /// throughout prefill — a stream that has gone quiet for
    /// [`AGENTIC_STREAM_IDLE_TIMEOUT_SECS`] has stalled, not merely taken its
    /// time. Compare mode does **not** set `return_progress`, which is why it
    /// keeps the old client rather than sharing this one; making the two agree
    /// means teaching compare to ask for progress frames first.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest` cannot build the client (extremely rare —
    /// only fails on TLS initialisation errors).
    pub fn build_agentic_http_client() -> Result<reqwest::Client> {
        Self::agentic_http_client(Duration::from_secs(AGENTIC_STREAM_IDLE_TIMEOUT_SECS))
    }

    /// [`Self::build_agentic_http_client`] with the idle timeout supplied, so a
    /// test can exercise the real builder in milliseconds instead of ninety
    /// seconds. There is deliberately no total-request deadline to inject —
    /// the absence of one is the property under test.
    fn agentic_http_client(stream_idle_timeout: Duration) -> Result<reqwest::Client> {
        reqwest::Client::builder()
            .read_timeout(stream_idle_timeout)
            .connect_timeout(Duration::from_secs(AGENTIC_CONNECT_TIMEOUT_SECS))
            .pool_idle_timeout(Duration::from_secs(AGENTIC_POOL_IDLE_TIMEOUT_SECS))
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build agentic eval HTTP client: {e}"))
    }
}

#[cfg(test)]
mod client_tests {
    use std::time::{Duration, Instant};

    use futures_util::StreamExt as _;
    use tokio::io::AsyncWriteExt as _;
    use tokio::net::TcpListener;

    use super::BenchmarkDeps;

    /// Serve one SSE response, writing `frames` with `gap` between them, then
    /// hold the socket open and silent forever.
    ///
    /// Open-ended on purpose: no `Content-Length`, no terminating chunk. That
    /// is the shape of a real llama-server stream, and it is the shape a
    /// total-request deadline cannot tell apart from a hang.
    async fn sse_stub(frames: usize, gap: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
                )
                .await
                .expect("headers");
            for _ in 0..frames {
                tokio::time::sleep(gap).await;
                if socket.write_all(b"data: {}\n\n").await.is_err() {
                    return;
                }
                socket.flush().await.ok();
            }
            // Go quiet without closing. A client that only knows about total
            // deadlines cannot distinguish this from the loop above.
            tokio::time::sleep(Duration::from_secs(3600)).await;
        });
        format!("http://{addr}/")
    }

    async fn drain(url: &str, client: &reqwest::Client) -> (Result<(), String>, Duration) {
        let started = Instant::now();
        let response = client.get(url).send().await.expect("send");
        let mut stream = response.bytes_stream();
        let mut outcome = Ok(());
        while let Some(chunk) = stream.next().await {
            if let Err(e) = chunk {
                // `{:#}` so the source beneath reqwest's blanket decode wrapper
                // is visible — the same reason `stream.rs` stopped using `{}`.
                outcome = Err(format!("{:#}", anyhow::Error::new(e)));
                break;
            }
        }
        (outcome, started.elapsed())
    }

    /// A stalled stream must fail on its own idle clock, not ten minutes later.
    #[tokio::test]
    async fn a_silent_stream_fails_on_the_idle_timeout() {
        let url = sse_stub(1, Duration::from_millis(10)).await;
        let client =
            BenchmarkDeps::agentic_http_client(Duration::from_millis(300)).expect("client");

        let (outcome, elapsed) = drain(&url, &client).await;

        let err = outcome.expect_err("a silent stream must not read as a clean end");
        assert!(
            elapsed < Duration::from_secs(2),
            "idle stream should fail on its own clock, took {elapsed:?}"
        );
        // The whole point of preserving the chain: "error decoding response
        // body" alone would not distinguish this from a connection reset.
        assert!(
            err.to_lowercase().contains("time"),
            "the error must name the timeout, got: {err}"
        );
    }

    /// **The regression this file exists to prevent.**
    ///
    /// A stream that keeps producing must survive past any fixed total
    /// deadline. The 2026-08-28 eval lost five runs because the benchmark
    /// client carried `.timeout(600s)`, which reqwest applies until the
    /// *response body finishes* — so a long agentic stream was severed
    /// mid-body and reported as a decode failure.
    ///
    /// This test fails if a total-request deadline is ever reintroduced: the
    /// stub keeps sending for ~1.2s, well past the 300ms idle timeout, and only
    /// a client without a total deadline can read it.
    #[tokio::test]
    async fn a_slow_but_progressing_stream_is_not_severed() {
        let url = sse_stub(12, Duration::from_millis(100)).await;
        let client =
            BenchmarkDeps::agentic_http_client(Duration::from_millis(300)).expect("client");

        let (outcome, elapsed) = drain(&url, &client).await;

        // It still ends in the idle timeout once the stub goes quiet — but only
        // *after* outliving several times its own idle window, which a total
        // deadline of the same size would not have allowed.
        assert!(
            elapsed > Duration::from_secs(1),
            "a progressing stream was cut short after {elapsed:?}; a total-request \
             deadline has been reintroduced"
        );
        assert!(
            outcome.is_err(),
            "the stub goes silent, so this ends in the idle timeout"
        );
    }
}
