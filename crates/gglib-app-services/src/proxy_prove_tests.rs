//! The proof request against a stand-in endpoint: who answered is read from
//! the answer, the key travels as the bearer and nowhere else, a refusal
//! carries the endpoint's words, and a listener that never comes is said.
//!
//! The stand-in reads each request to its end before answering, as a peer
//! must ([`gglib_test_http`]).

use std::io::Write as _;
use std::time::Duration;

use tokio::net::TcpListener;

use super::*;

/// A stand-in endpoint that answers `/health` and then one completion with
/// `status` and `body`; hands back the completion request it read.
async fn endpoint(
    status: &'static str,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<gglib_test_http::Request>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let served = tokio::spawn(async move {
        loop {
            let (request, stream) = gglib_test_http::accept(&listener).await.expect("a request");
            let mut stream = stream.into_std().expect("a blocking stream");
            stream.set_nonblocking(false).ok();
            if request.line().starts_with("GET /health") {
                let _ = write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok"
                );
                continue;
            }
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            return request;
        }
    });
    (base, served)
}

#[tokio::test]
async fn the_answer_names_who_answered_and_the_key_travels_as_the_bearer() {
    let (base, served) = endpoint("200 OK", r#"{"model":"qwen3-8b","choices":[]}"#).await;
    let outcome = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "Qwen3 8B",
        Some("k3y-5f3a"),
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect("answered");
    // A pinned endpoint answers with its pin whatever was asked for, so the
    // answer's model is the one reported, and the request's is kept beside it.
    assert_eq!(outcome.answered_by, "qwen3-8b");
    assert_eq!(outcome.asked_for, "Qwen3 8B");

    let request = served.await.unwrap();
    assert!(
        request.line().starts_with("POST /v1/chat/completions"),
        "{}",
        request.line()
    );
    assert_eq!(request.header("authorization"), Some("Bearer k3y-5f3a"));
    let sent: serde_json::Value = serde_json::from_str(&request.body_text()).unwrap();
    assert_eq!(sent["model"], "Qwen3 8B");
    assert_eq!(sent["max_tokens"], 1);
    assert_eq!(sent["stream"], false);
}

#[tokio::test]
async fn without_a_key_none_is_sent_and_an_answer_without_a_model_names_the_one_asked_for() {
    let (base, served) = endpoint("200 OK", r#"{"choices":[]}"#).await;
    let outcome = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect("answered");
    assert_eq!(outcome.answered_by, "m");
    assert_eq!(served.await.unwrap().header("authorization"), None);
}

#[tokio::test]
async fn a_refusal_carries_the_endpoints_status_and_words() {
    let (base, _served) = endpoint("404 Not Found", r#"{"error":"model not found: m"}"#).await;
    let refusal = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        Duration::from_secs(5),
        Duration::from_secs(5),
    )
    .await
    .expect_err("refused");
    assert_eq!(
        refusal.reason,
        r#"404 Not Found: {"error":"model not found: m"}"#
    );
}

#[tokio::test]
async fn a_listener_that_never_comes_is_said_within_the_bind_timeout() {
    // A port nothing listens on: bound and dropped, so it is free.
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let base = format!("http://{free}/v1");
    let refusal = prove(
        &gglib_proxy::loopback::client(),
        &base,
        "m",
        None,
        Duration::from_millis(300),
        Duration::from_secs(1),
    )
    .await
    .expect_err("no listener");
    assert!(
        refusal.reason.contains("did not answer /health"),
        "{}",
        refusal.reason
    );
}

/// The answer's keys are the contract's, which the CLI's mirror is held to.
#[test]
fn the_answer_carries_exactly_the_fields_the_contract_names() {
    let outcome = ProofOutcome {
        answered_by: "m".to_owned(),
        asked_for: "m".to_owned(),
        elapsed_ms: 1,
    };
    let value = serde_json::to_value(outcome).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    let mut named = gglib_core::contracts::http::daemon::PROXY_PROVE_FIELDS.to_vec();
    named.sort_unstable();
    assert_eq!(keys, named);
}
