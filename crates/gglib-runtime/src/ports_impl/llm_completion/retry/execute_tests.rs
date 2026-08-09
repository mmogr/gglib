//! Retry-loop tests, driven against a real socket.
//!
//! Backoff *arithmetic* is proven exactly in `gglib_core::retry`'s own suite,
//! which needs no clock. These tests prove the surrounding behaviour — what
//! gets retried, how often, and what reaches the observer — so they use a
//! policy with millisecond delays rather than freezing time. Pausing the clock
//! is unreliable here: auto-advance treats a task parked on socket I/O as idle,
//! which would fire the send timeout spuriously.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use gglib_core::domain::agent::AgentMessage;
use gglib_core::ports::{LlmCompletionPort, RetryObserver};
use gglib_core::retry::RetryPolicy;
use reqwest::{Client, Response};

use super::super::LlmCompletionAdapter;
use super::send_with_retry;
use super::test_server::{TestServer, admission_timeout_body, json, json_with, sse};

/// Delays small enough that a full sequence costs single-digit milliseconds.
fn fast_policy(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        initial_backoff: Duration::from_millis(1),
        max_backoff: Duration::from_millis(5),
        total_deadline: Duration::from_secs(5),
    }
}

/// One SSE frame, enough for the stream decoder to produce an event.
fn ok_stream() -> String {
    sse(&[r#"{"choices":[{"delta":{"content":"ok"}}]}"#])
}

fn unavailable() -> String {
    json(503, "Service Unavailable", &admission_timeout_body())
}

async fn send_to(
    server: &TestServer,
    policy: &RetryPolicy,
    observer: Option<&Arc<dyn RetryObserver>>,
) -> Result<Response> {
    let url = format!("{}/v1/chat/completions", server.base_url);
    send_with_retry(
        &Client::new(),
        &url,
        &serde_json::json!({"model": "test"}),
        Duration::from_secs(5),
        policy,
        observer,
    )
    .await
}

async fn send(server: &TestServer, policy: &RetryPolicy) -> Result<Response> {
    send_to(server, policy, None).await
}

/// Captures what the adapter reported. Cloning shares the buffers, so the test
/// keeps a handle to the data after the trait object is handed over.
#[derive(Clone, Default)]
struct Recorder {
    retries: Arc<Mutex<Vec<(u32, Duration, String)>>>,
    exhausted: Arc<Mutex<Vec<(u32, String)>>>,
}

impl Recorder {
    fn retries(&self) -> Vec<(u32, Duration, String)> {
        self.retries.lock().expect("recorder mutex").clone()
    }

    fn exhausted(&self) -> Vec<(u32, String)> {
        self.exhausted.lock().expect("recorder mutex").clone()
    }

    /// Hand out the trait object while keeping this handle on the data.
    fn as_observer(&self) -> Arc<dyn RetryObserver> {
        Arc::new(self.clone())
    }
}

impl RetryObserver for Recorder {
    fn on_retry(&self, attempt: u32, delay: Duration, reason: &str) {
        self.retries
            .lock()
            .expect("recorder mutex")
            .push((attempt, delay, reason.to_owned()));
    }

    fn on_exhausted(&self, attempts: u32, _elapsed: Duration, reason: &str) {
        self.exhausted
            .lock()
            .expect("recorder mutex")
            .push((attempts, reason.to_owned()));
    }
}

// =============================================================================
// What gets retried
// =============================================================================

#[tokio::test]
async fn recovers_after_a_transient_admission_timeout() {
    let retry_after = [("Retry-After", "5")];
    let server = TestServer::start(vec![
        json_with(
            503,
            "Service Unavailable",
            &admission_timeout_body(),
            &retry_after,
        ),
        json_with(
            503,
            "Service Unavailable",
            &admission_timeout_body(),
            &retry_after,
        ),
        ok_stream(),
    ])
    .await;

    let response = send(&server, &fast_policy(4))
        .await
        .expect("should recover");

    assert!(response.status().is_success());
    assert_eq!(server.request_count(), 3, "two failures then one success");
}

#[tokio::test]
async fn a_terminal_error_is_not_retried() {
    let body = r#"{"error":{"message":"no such model","type":"invalid_request_error","code":"model_not_found"}}"#;
    let server = TestServer::start(vec![json(404, "Not Found", body)]).await;

    let error = send(&server, &fast_policy(4))
        .await
        .expect_err("404 must not be retried");

    assert_eq!(server.request_count(), 1, "exactly one attempt");
    assert!(
        error.to_string().contains("invalid_request_error"),
        "the structured type should survive into the message: {error}"
    );
}

#[tokio::test]
async fn gives_up_after_max_attempts() {
    let server = TestServer::start(vec![unavailable()]).await;

    let error = send(&server, &fast_policy(4))
        .await
        .expect_err("a permanently contended upstream must fail");

    assert_eq!(server.request_count(), 4, "max_attempts is the ceiling");
    assert!(
        error.to_string().contains("attempts exhausted"),
        "give-up reason should be reported: {error}"
    );
}

#[tokio::test]
async fn a_policy_of_one_attempt_disables_retrying() {
    let server = TestServer::start(vec![unavailable()]).await;

    send(&server, &fast_policy(1))
        .await
        .expect_err("still fails");

    assert_eq!(server.request_count(), 1, "--no-retry semantics");
}

// =============================================================================
// Classification without a gglib error body
// =============================================================================

#[tokio::test]
async fn a_bare_503_falls_back_to_status_semantics() {
    // A llama-server reached directly sends nothing this crate can parse.
    let server = TestServer::start(vec![
        json(503, "Service Unavailable", "upstream is busy"),
        ok_stream(),
    ])
    .await;

    let response = send(&server, &fast_policy(4))
        .await
        .expect("should recover");

    assert!(response.status().is_success());
    assert_eq!(server.request_count(), 2, "status alone made it retryable");
}

#[tokio::test]
async fn a_bare_500_is_terminal() {
    let server = TestServer::start(vec![json(500, "Internal Server Error", "boom")]).await;

    send(&server, &fast_policy(4))
        .await
        .expect_err("500 is not retryable");

    assert_eq!(server.request_count(), 1);
}

/// The wired path from header to sleep, not the arithmetic — that is proven
/// exactly, and without a clock, by `retry::policy_tests`.
///
/// # Why this reads the observer rather than the clock
///
/// It used to assert that the whole `send` finished inside a second, which is
/// a wall-clock measurement of two HTTP round trips plus a 5ms backoff. Under
/// parallel test load that took 1.1–1.25s on this machine and failed, having
/// detected nothing about clamping — the backoff was never the reason it went
/// over. The observer is handed the delay the loop is about to sleep for, so
/// asserting on that measures the value under test directly and cannot be
/// perturbed by a busy machine.
#[tokio::test]
async fn an_absurd_retry_after_is_clamped() {
    // A full day, which must not park the request for a full day.
    let server = TestServer::start(vec![
        json_with(
            503,
            "Service Unavailable",
            &admission_timeout_body(),
            &[("Retry-After", "86400")],
        ),
        ok_stream(),
    ])
    .await;
    let recorder = Recorder::default();
    let policy = fast_policy(4);

    send_to(&server, &policy, Some(&recorder.as_observer()))
        .await
        .expect("should recover");

    assert_eq!(server.request_count(), 2);
    let retries = recorder.retries();
    assert_eq!(retries.len(), 1, "one backoff, and its delay is the subject");
    // A server hint is a floor clamped to `max_backoff`, plus a jitter spread
    // drawn from `initial_backoff` — so their sum is the whole ceiling.
    let ceiling = policy.max_backoff + policy.initial_backoff;
    assert!(
        retries[0].1 <= ceiling,
        "clamped to max_backoff, not honoured literally: about to sleep {:?}",
        retries[0].1
    );
}

// =============================================================================
// Observer reporting
// =============================================================================

#[tokio::test]
async fn the_observer_sees_every_backoff() {
    let server = TestServer::start(vec![unavailable(), unavailable(), ok_stream()]).await;
    let recorder = Recorder::default();

    send_to(&server, &fast_policy(4), Some(&recorder.as_observer()))
        .await
        .expect("should recover");

    let retries = recorder.retries();
    assert_eq!(retries.len(), 2, "one notice per backoff");
    assert_eq!(retries[0].0, 1, "attempts are reported 1-based");
    assert_eq!(retries[1].0, 2);
    assert!(
        retries[0].2.contains("service_unavailable"),
        "the notice carries the structured cause: {}",
        retries[0].2
    );
    assert!(
        recorder.exhausted().is_empty(),
        "a recovered request never reports exhaustion"
    );
}

#[tokio::test]
async fn the_observer_is_told_when_the_budget_runs_out() {
    let server = TestServer::start(vec![unavailable()]).await;
    let recorder = Recorder::default();

    send_to(&server, &fast_policy(3), Some(&recorder.as_observer()))
        .await
        .expect_err("should fail");

    assert_eq!(
        recorder.retries().len(),
        2,
        "three attempts means two backoffs"
    );
    assert_eq!(
        recorder.exhausted().len(),
        1,
        "exhaustion is reported exactly once"
    );
}

// =============================================================================
// The idempotency window
// =============================================================================

#[tokio::test]
async fn a_stream_that_starts_is_never_retried() {
    // The guard this whole design rests on: once a 2xx is returned, the body is
    // the user's tokens. A retry past this point would duplicate them.
    let server = TestServer::start(vec![sse(&[
        r#"{"choices":[{"delta":{"content":"one"}}]}"#,
        r#"{"choices":[{"delta":{"content":"two"}}]}"#,
    ])])
    .await;

    let adapter =
        LlmCompletionAdapter::new(server.base_url.clone(), None).with_retry_policy(fast_policy(4));
    let messages = [AgentMessage::User {
        content: "hello".to_owned(),
    }];

    let stream = adapter
        .chat_stream(&messages, &[])
        .await
        .expect("stream should open");
    let events: Vec<_> = stream.collect().await;

    assert_eq!(
        server.request_count(),
        1,
        "consuming a stream must never re-issue the request"
    );
    assert!(
        !events.is_empty(),
        "the harness should have produced decodable frames"
    );
}
