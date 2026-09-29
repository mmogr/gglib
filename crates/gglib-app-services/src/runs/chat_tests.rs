//! The chat executor against a fake proxy on loopback.

use std::sync::Arc;
use std::time::Duration;

use gglib_core::domain::runs::{RunInfo, RunStatus};
use gglib_core::ports::{RunScope, RunsPort};
use serde_json::{Value, json};

use super::RunRegistry;
use super::chat::ChatExecutor;
use super::door::{LocalProxy, ProxyDoor};
use super::fake_proxy::{FakeProxy, STREAM_HEAD, door};
use super::test_executor::{HandClock, drain, next};

const LOCAL: RunScope = RunScope::Local;

fn registry_on(door: Arc<dyn ProxyDoor>) -> RunRegistry {
    RunRegistry::new(
        Arc::new(ChatExecutor::new(door)),
        HandClock::at(1_000).clock(),
    )
}

fn request() -> Value {
    json!({
        "model": "qwen",
        "stream": false,
        "messages": [{ "role": "user", "content": "hello" }],
    })
}

async fn finished(runs: &RunRegistry, id: &str) -> (Vec<String>, RunInfo) {
    let (frames, end) = drain(runs.events(&LOCAL, id, 0).unwrap()).await;
    let frames = frames.into_iter().map(|(_, data)| data).collect();
    (frames, end.expect("the run ends"))
}

#[tokio::test]
async fn each_data_payload_is_logged_verbatim_and_done_is_not() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, Some("k-local")));
    let created = runs.create(LOCAL, "r1", request()).unwrap();
    assert_eq!(created.info.model.as_deref(), Some("qwen"));

    let seen = (&mut proxy.seen).await.expect("the proxy was asked");
    assert!(
        seen.head
            .starts_with("POST /v1/chat/completions HTTP/1.1\r\n")
    );
    let auth = seen
        .head
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("authorization:"));
    assert_eq!(auth, Some("authorization: Bearer k-local"));
    assert_eq!(seen.body["stream"], json!(true), "streaming is forced on");
    assert_eq!(seen.body["return_progress"], json!(true));
    assert_eq!(seen.body["model"], json!("qwen"));
    assert_eq!(seen.body["messages"], request()["messages"]);

    proxy.say(STREAM_HEAD);
    proxy.say("data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n");
    proxy.say(": keep-alive\n\ndata:{\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\n");
    proxy.say("data: [DONE]\n\n");
    proxy.close();

    let (frames, info) = finished(&runs, "r1").await;
    assert_eq!(
        frames,
        [
            "{\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}",
            "{\"choices\":[{\"delta\":{\"content\":\" there\"}}]}",
        ]
    );
    assert_eq!(info.status, RunStatus::Completed);
    assert_eq!(info.last_seq, 2);
    assert_eq!(info.error, None);
}

#[tokio::test]
async fn a_proxy_that_wants_no_key_is_sent_none() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();

    let seen = (&mut proxy.seen).await.unwrap();

    assert!(!seen.head.to_ascii_lowercase().contains("authorization:"));
}

#[tokio::test]
async fn a_proxy_bound_to_every_interface_is_dialled_on_loopback() {
    let mut proxy = FakeProxy::start().await;
    let wildcard = std::net::SocketAddr::from(([0, 0, 0, 0], proxy.addr.port()));
    let runs = registry_on(door(wildcard, None));
    runs.create(LOCAL, "r1", request()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), &mut proxy.seen)
        .await
        .expect("the request reached the proxy on 127.0.0.1")
        .unwrap();
}

#[tokio::test]
async fn the_run_is_queued_until_the_proxy_answers_then_in_progress() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();
    (&mut proxy.seen).await.unwrap();
    assert_eq!(runs.get(&LOCAL, "r1").unwrap().status, RunStatus::Queued);

    proxy.say(STREAM_HEAD);
    proxy.say("data: {}\n\n");
    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();
    next(&mut events).await;

    assert_eq!(
        runs.get(&LOCAL, "r1").unwrap().status,
        RunStatus::InProgress
    );
}

#[tokio::test]
async fn a_refusal_fails_the_run_with_the_proxys_own_code_and_message() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();
    (&mut proxy.seen).await.unwrap();
    let body = r#"{"error":{"message":"Model 'qwen' not found","type":"invalid_request_error","code":"model_not_found"}}"#;
    proxy.say(format!(
        "HTTP/1.1 404 Not Found\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    ));
    proxy.close();

    let (frames, info) = finished(&runs, "r1").await;

    assert!(frames.is_empty());
    assert_eq!(info.status, RunStatus::Failed);
    let error = info.error.expect("an error");
    assert_eq!(error.code, "model_not_found");
    assert_eq!(error.message, "Model 'qwen' not found");
}

#[tokio::test]
async fn a_refusal_that_is_not_the_proxys_error_body_is_an_upstream_error() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();
    (&mut proxy.seen).await.unwrap();
    proxy.say("HTTP/1.1 502 Bad Gateway\r\ncontent-length: 3\r\nconnection: close\r\n\r\nbad");
    proxy.close();

    let (_, info) = finished(&runs, "r1").await;

    assert_eq!(
        info.error.map(|e| e.code).as_deref(),
        Some("upstream_error")
    );
}

#[tokio::test]
async fn a_reply_that_ends_without_done_fails_and_keeps_what_it_logged() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();
    (&mut proxy.seen).await.unwrap();
    proxy.say(STREAM_HEAD);
    proxy.say("data: {\"n\":1}\n\n");
    proxy.close();

    let (frames, info) = finished(&runs, "r1").await;

    assert_eq!(frames, ["{\"n\":1}"]);
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(
        info.error.map(|e| e.code).as_deref(),
        Some("upstream_error")
    );
}

#[tokio::test]
async fn an_error_frame_is_logged_and_fails_the_run_with_its_code() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();
    (&mut proxy.seen).await.unwrap();
    let frame = r#"{"error":{"message":"upstream did not respond within 60s","type":"server_error","code":"upstream_timeout"}}"#;
    proxy.say(STREAM_HEAD);
    proxy.say(format!("data: {frame}\n\n"));

    let (frames, info) = finished(&runs, "r1").await;

    assert_eq!(frames, [frame]);
    assert_eq!(info.status, RunStatus::Failed);
    let error = info.error.expect("an error");
    assert_eq!(error.code, "upstream_timeout");
    assert_eq!(error.message, "upstream did not respond within 60s");
}

#[tokio::test]
async fn cancelling_a_run_drops_its_connection_to_the_proxy() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();
    (&mut proxy.seen).await.unwrap();
    proxy.say(STREAM_HEAD);
    proxy.say("data: {}\n\n");
    let mut events = runs.events(&LOCAL, "r1", 0).unwrap();
    next(&mut events).await;

    runs.cancel(&LOCAL, "r1").unwrap();

    tokio::time::timeout(Duration::from_secs(5), &mut proxy.client_left)
        .await
        .expect("the connection is dropped")
        .unwrap();
    assert_eq!(runs.get(&LOCAL, "r1").unwrap().status, RunStatus::Cancelled);
}

#[tokio::test]
async fn a_proxy_that_is_not_running_fails_the_run_and_is_not_started() {
    let (core, proxy) = crate::test_support::test_core_and_proxy().await;
    let runs = registry_on(Arc::new(LocalProxy {
        proxy: Arc::clone(&proxy),
        core,
    }));
    runs.create(LOCAL, "r1", request()).unwrap();

    let (_, info) = finished(&runs, "r1").await;

    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(
        info.error.map(|e| e.code).as_deref(),
        Some("proxy_not_running")
    );
    assert!(matches!(
        proxy.status().await,
        gglib_runtime::proxy::ProxyStatus::Stopped
    ));
}

/// A refusal's head is not an answer: the run stays queued while its body
/// is still coming, then fails.
#[tokio::test]
async fn a_refusal_whose_body_is_held_back_leaves_the_run_queued_until_it_fails() {
    let mut proxy = FakeProxy::start().await;
    let runs = registry_on(door(proxy.addr, None));
    runs.create(LOCAL, "r1", request()).unwrap();
    (&mut proxy.seen).await.unwrap();
    let body = r#"{"error":{"message":"busy","type":"server_error","code":"overloaded"}}"#;
    proxy.say(format!(
        "HTTP/1.1 503 Service Unavailable\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    ));

    // The head has had time to arrive and be read; the body has not been sent.
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(5)).await;
        assert_eq!(runs.get(&LOCAL, "r1").unwrap().status, RunStatus::Queued);
    }
    proxy.say(body);
    proxy.close();

    let (_, info) = finished(&runs, "r1").await;
    assert_eq!(info.status, RunStatus::Failed);
    assert_eq!(info.error.map(|e| e.code).as_deref(), Some("overloaded"));
}
