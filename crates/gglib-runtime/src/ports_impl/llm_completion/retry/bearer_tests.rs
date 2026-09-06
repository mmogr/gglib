//! The bearer reaches the wire exactly when one is set.
//!
//! The remote tunnel's loopback port is another machine's proxy and demands
//! its key; a llama-server on loopback demands nothing. Both cases run
//! through the same adapter, so both are pinned here against a server that
//! records what it was sent.

use std::time::Duration;

use gglib_core::retry::RetryPolicy;
use reqwest::Client;

use super::send_with_retry;
use super::test_server::{TestServer, sse};

async fn send(server: &TestServer, bearer: Option<&str>) {
    let url = format!("{}/v1/chat/completions", server.base_url);
    send_with_retry(
        &Client::new(),
        &url,
        bearer,
        &serde_json::json!({"model": "test"}),
        Duration::from_secs(5),
        &RetryPolicy::default(),
        None,
    )
    .await
    .expect("the scripted 200 is returned");
}

fn authorization_line(head: &str) -> Option<String> {
    head.lines()
        .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
        .map(str::to_owned)
}

#[tokio::test]
async fn a_bearer_is_sent_as_the_authorization_header() {
    let server = TestServer::start(vec![sse(&["[DONE]"])]).await;
    send(&server, Some("far-machine-key")).await;
    let heads = server.request_heads();
    assert_eq!(heads.len(), 1);
    assert_eq!(
        authorization_line(&heads[0]).as_deref(),
        Some("authorization: Bearer far-machine-key"),
        "{}",
        heads[0]
    );
}

#[tokio::test]
async fn no_bearer_means_no_authorization_header_at_all() {
    let server = TestServer::start(vec![sse(&["[DONE]"])]).await;
    send(&server, None).await;
    let heads = server.request_heads();
    assert_eq!(heads.len(), 1);
    assert!(authorization_line(&heads[0]).is_none(), "{}", heads[0]);
}
