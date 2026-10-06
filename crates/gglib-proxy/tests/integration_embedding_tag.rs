//! A model hand-tagged `Embedding` is an embedding model to the proxy, as it
//! is to the launch.
//!
//! A hand edit stores a tag as typed, and the launch passes `--embeddings` for
//! the tag in any case. The proxy routes by the same predicate
//! (`capability_tags::is_embedding`): were it to match the tag exactly, it
//! would refuse this model the embeddings its server serves, and forward it
//! the chat that server refuses, paying for a model swap to collect a 501.

mod fixtures;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use gglib_core::ports::ModelRuntimePort;

use fixtures::common::{
    CountingRuntime, spawn_mock_embeddings_upstream, spawn_proxy, spawn_proxy_with_runtime,
};

const MODEL: &str = "hand-tagged";

fn hand_edited_tags() -> Vec<String> {
    vec!["Embedding".to_string()]
}

#[tokio::test]
async fn an_embeddings_request_for_a_model_tagged_in_another_case_reaches_the_upstream() {
    let cancel = CancellationToken::new();
    let (upstream, last_body) = spawn_mock_embeddings_upstream(cancel.clone(), None).await;
    let (base, proxy_cancel) = spawn_proxy(upstream, MODEL, hand_edited_tags()).await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&json!({ "model": MODEL, "input": "hello" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"][0]["embedding"].as_array().unwrap().len(), 3);
    assert!(
        last_body.lock().await.is_some(),
        "the upstream must be asked for the embedding"
    );

    proxy_cancel.cancel();
    cancel.cancel();
}

#[tokio::test]
async fn a_chat_completion_for_a_model_tagged_in_another_case_never_reaches_the_runtime() {
    let cancel = CancellationToken::new();
    let (upstream, _) = spawn_mock_embeddings_upstream(cancel.clone(), None).await;
    let (runtime, admit_calls) = CountingRuntime::new(upstream, MODEL);
    let (base, proxy_cancel) = spawn_proxy_with_runtime(
        runtime as Arc<dyn ModelRuntimePort>,
        MODEL,
        hand_edited_tags(),
    )
    .await;

    let resp = Client::new()
        .post(format!("{base}/v1/chat/completions"))
        .json(&json!({
            "model": MODEL,
            "messages": [{ "role": "user", "content": "hi" }],
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "embedding_model_cannot_chat");
    assert_eq!(
        admit_calls.load(Ordering::SeqCst),
        0,
        "a chat request must not swap in a model that can only refuse it"
    );

    proxy_cancel.cancel();
    cancel.cancel();
}
