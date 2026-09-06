//! Classification tests, against the bodies the two real upstreams write.
//!
//! Every literal below is copied verbatim from the program that emits it —
//! modelpipe 0.2.0's `refusal.rs` (the version gglib pins) and this proxy's
//! own `ErrorResponse`. That is the point of the file: the interesting
//! failures here were never about the logic, they were about a wire shape
//! that did not look the way the struct said it did, and a test that
//! paraphrased the body would have gone green against the bug.

use chrono::Utc;
use reqwest::Client;

use super::classify::{Failure, classify};
use super::test_server::{TestServer, json};

// modelpipe's edge writes `{"error":{"message":…,"code":…}}` — no `type` key —
// and answers all three of its gateway refusals with 502, so neither the
// discriminant this proxy uses nor the status can tell them apart on its own.

/// The connect side has no tunnel: the peer is away, or `keep_connected` is
/// dialling a replacement after the laptop changed networks.
const TUNNEL_UNAVAILABLE: &str = r#"{"error":{"message":"no tunnel to the serving side is connected right now","code":"tunnel_unavailable"}}"#;

/// The serving side reached for its model server and found nothing there.
const BACKEND_UNREACHABLE: &str = r#"{"error":{"message":"the serving side could not reach its backend","code":"backend_unreachable"}}"#;

/// The edge's own 401, written before the backend is contacted at all.
const INVALID_API_KEY: &str =
    r#"{"error":{"message":"invalid or missing bearer token","code":"invalid_api_key"}}"#;

/// This proxy's admission timeout — the shape with every field filled in.
const ADMISSION_TIMEOUT: &str = r#"{"error":{"message":"waited without reaching the front of the queue","type":"service_unavailable","code":"admission_timeout"}}"#;

/// Serve `body` once and hand the response to `classify`.
///
/// A real socket rather than a hand-built [`reqwest::Response`]: the crate has
/// no way to construct one directly, and the scripted server the retry tests
/// already use costs a single ephemeral port.
async fn classify_body(status: u16, reason: &str, body: &str) -> Failure {
    let server = TestServer::start(vec![json(status, reason, body)]).await;
    let response = Client::new()
        .get(&server.base_url)
        .send()
        .await
        .expect("the scripted server answers");

    classify(response, Utc::now())
        .await
        .expect_err("a non-2xx is always a failure")
}

/// The one that proves the roaming case.
///
/// A tunnel modelpipe is re-dialling is up again in seconds, and the request
/// has not been sent anywhere yet, so repeating it is both safe and the whole
/// reason `keep_connected` exists. Nothing else in this classifier rescues it:
/// the status is 502, which the status-only fallback deliberately does not
/// retry.
#[tokio::test]
async fn a_tunnel_being_redialled_is_retryable() {
    let failure = classify_body(502, "Bad Gateway", TUNNEL_UNAVAILABLE).await;

    assert!(
        matches!(failure, Failure::Retryable { .. }),
        "a tunnel between connections must not kill the turn: {failure:?}"
    );
}

/// The far machine's model server is a different problem from the tunnel to
/// it, and repeating a request cannot start a process that is not running.
#[tokio::test]
async fn a_backend_the_serving_side_cannot_reach_is_terminal() {
    let failure = classify_body(502, "Bad Gateway", BACKEND_UNREACHABLE).await;

    assert!(
        matches!(failure, Failure::Terminal { .. }),
        "only the tunnel's own 502 is retryable: {failure:?}"
    );
}

/// A refusal with no `type` used to fail deserialization outright, which sent
/// the whole body through the raw-text arm — so the user was shown the JSON
/// instead of the sentence inside it. The sentence is what a person can act
/// on, and the `code` stands in for the missing discriminant as its label.
#[tokio::test]
async fn a_refusal_with_no_type_reads_as_the_sentence_modelpipe_wrote() {
    let reason = classify_body(401, "Unauthorized", INVALID_API_KEY)
        .await
        .reason()
        .to_owned();

    assert_eq!(
        reason, "401 Unauthorized invalid_api_key: invalid or missing bearer token",
        "the code labels the message when there is no type to"
    );
}

/// A body carrying neither discriminant is still an error body, and the label
/// that is not there must not print as an empty one.
#[tokio::test]
async fn a_body_that_names_no_discriminant_gets_no_stray_separator() {
    let reason = classify_body(
        500,
        "Internal Server Error",
        r#"{"error":{"message":"boom"}}"#,
    )
    .await
    .reason()
    .to_owned();

    assert_eq!(reason, "500 Internal Server Error: boom");
}

/// The proxy's own bodies are unaffected by any of the above: `type` still
/// decides, and still labels the message.
#[tokio::test]
async fn the_proxys_own_admission_timeout_still_classifies_on_its_type() {
    let failure = classify_body(503, "Service Unavailable", ADMISSION_TIMEOUT).await;

    assert!(
        matches!(failure, Failure::Retryable { .. }),
        "an admission timeout is the original retryable case: {failure:?}"
    );
    assert!(
        failure.reason().contains("service_unavailable"),
        "the type outranks the code as the label: {}",
        failure.reason()
    );
}

/// A body that is not this shape at all — llama-server's own errors, an HTML
/// gateway page — keeps falling through to the status, unchanged.
#[tokio::test]
async fn an_unparseable_body_still_falls_back_to_the_status() {
    let failure = classify_body(503, "Service Unavailable", "<html>upstream down</html>").await;

    assert!(
        matches!(failure, Failure::Retryable { .. }),
        "503 is retryable on status alone: {failure:?}"
    );
    assert!(
        failure.reason().contains("<html>"),
        "the raw body is what there is to report: {}",
        failure.reason()
    );
}
