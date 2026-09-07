//! Classification tests, against the bodies the two real upstreams write.
//!
//! No body that a real upstream writes is spelled out here. That is the point
//! of the file: the interesting failures were never about the logic, they
//! were about a wire shape that did not look the way the struct said it did,
//! and a test that paraphrased the body would have gone green against the
//! bug. So each of those has a single home in
//! [`test_server`](super::test_server) — the modelpipe ones copied verbatim
//! from version 0.2.0's `refusal.rs`, this proxy's own built through
//! [`ErrorResponse`](gglib_proxy::models::ErrorResponse) — and a modelpipe
//! bump has one place to update however many test files read them.
//!
//! The literals that remain are deliberately *not* anybody's wire shape.
//! `{"error":{"message":"boom"}}` and an HTML blob are the arguments to the
//! rules under test — a body naming no discriminant, a body that is not this
//! envelope at all — and a reader has to see them at the assertion to know
//! what is being asserted. Naming them would hide the input, not share it.

use chrono::Utc;
use reqwest::Client;

use super::classify::{Failure, classify};
use super::test_server::{
    TestServer, admission_timeout_body, edge_backend_unreachable_body, edge_invalid_api_key_body,
    edge_tunnel_unavailable_body, json,
};

// modelpipe's edge writes `{"error":{"message":…,"code":…}}` — no `type` key —
// and answers all three of its gateway refusals with 502, so neither the
// discriminant this proxy uses nor the status can tell them apart on its own.

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
    let failure = classify_body(502, "Bad Gateway", &edge_tunnel_unavailable_body()).await;

    assert!(
        matches!(failure, Failure::Retryable { .. }),
        "a tunnel between connections must not kill the turn: {failure:?}"
    );
}

/// The far machine's model server is a different problem from the tunnel to
/// it, and repeating a request cannot start a process that is not running.
#[tokio::test]
async fn a_backend_the_serving_side_cannot_reach_is_terminal() {
    let failure = classify_body(502, "Bad Gateway", &edge_backend_unreachable_body()).await;

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
    let reason = classify_body(401, "Unauthorized", &edge_invalid_api_key_body())
        .await
        .reason()
        .to_owned();

    assert_eq!(
        reason, "401 Unauthorized invalid_api_key: invalid or missing bearer token",
        "the code labels the message when there is no type to"
    );
}

/// The `code` survives classification as itself, not only folded into the
/// rendered reason.
///
/// A caller that has to act on one specific condition cannot read the term
/// back out of the sentence — that is the text matching this module opens by
/// forbidding — so the term has to travel beside it. The three cases are the
/// three a body can be in: a code that was written, a parseable body that
/// wrote none, and a body that was never this shape at all.
#[tokio::test]
async fn the_code_the_body_wrote_survives_beside_the_rendered_reason() {
    assert_eq!(
        classify_body(401, "Unauthorized", &edge_invalid_api_key_body())
            .await
            .code(),
        Some("invalid_api_key")
    );
    assert_eq!(
        classify_body(
            500,
            "Internal Server Error",
            r#"{"error":{"message":"boom"}}"#
        )
        .await
        .code(),
        None,
        "a body that wrote no code has none to carry"
    );
    assert_eq!(
        classify_body(503, "Service Unavailable", "<html>upstream down</html>")
            .await
            .code(),
        None,
        "nothing structured was parsed, so nothing structured is claimed"
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

/// The narrowing that defaulting `type` would otherwise have caused.
///
/// Before the default, this body failed to deserialize and reached the status
/// arm, where 503 is retryable. Parsing it must not change the answer: the
/// body names no discriminant, so it asserts nothing about retrying and the
/// status is still what decides. The 500 case above cannot catch this — 500
/// is terminal on both paths, so it stays green either way.
#[tokio::test]
async fn a_body_that_names_no_discriminant_still_retries_on_the_status() {
    let failure = classify_body(
        503,
        "Service Unavailable",
        r#"{"error":{"message":"busy"}}"#,
    )
    .await;

    assert!(
        matches!(failure, Failure::Retryable { .. }),
        "a parseable body with nothing to say must not outrank the status: {failure:?}"
    );
}

/// The proxy's own bodies are unaffected by any of the above: `type` still
/// decides, and still labels the message.
#[tokio::test]
async fn the_proxys_own_admission_timeout_still_classifies_on_its_type() {
    let failure = classify_body(503, "Service Unavailable", &admission_timeout_body()).await;

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
