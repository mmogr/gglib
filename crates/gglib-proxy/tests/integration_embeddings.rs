//! `POST /v1/embeddings` over real HTTP.
//!
//! The endpoint is a pass-through by design: the proxy reads `model` and
//! nothing else out of the body, so the tests that matter assert on what the
//! *upstream* received, not on what the handler parsed. That is the only way
//! to prove a client's `input` — a bare string in one call, an array in the
//! next — reaches llama-server unreshaped.
//!
//! The refusal tests are the other half. `--embeddings` restricts a
//! llama-server to embeddings, so gglib only passes it for a model tagged
//! `embedding`. Asking a chat model to embed can therefore never succeed, and
//! forwarding the request anyway would evict whatever is serving chat to spawn
//! a server that replies 501 — a swap that costs a model load and buys the
//! client nothing. The proxy refuses first.

mod fixtures;

use reqwest::Client;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use fixtures::common::{spawn_mock_embeddings_upstream, spawn_proxy};

const EMBED_MODEL: &str = "embed-model";
const CHAT_MODEL: &str = "chat-model";

fn embedding_tags() -> Vec<String> {
    vec!["embedding".to_string()]
}

#[tokio::test]
async fn string_input_reaches_the_upstream_unchanged() {
    let cancel = CancellationToken::new();
    let (upstream, last_body) = spawn_mock_embeddings_upstream(cancel.clone(), None).await;
    let (base, proxy_cancel) = spawn_proxy(upstream, EMBED_MODEL, embedding_tags()).await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&json!({ "model": EMBED_MODEL, "input": "hello world" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["data"][0]["embedding"].as_array().unwrap().len(), 3);

    let forwarded: Value = serde_json::from_slice(
        &last_body
            .lock()
            .await
            .clone()
            .expect("upstream saw a request"),
    )
    .unwrap();
    assert_eq!(forwarded["input"], json!("hello world"));

    proxy_cancel.cancel();
    cancel.cancel();
}

/// The array form is not a separate code path in the proxy — that is exactly
/// the claim being pinned. Nothing deserializes `input`, so a shape the
/// handler has never seen still arrives intact.
#[tokio::test]
async fn array_input_reaches_the_upstream_unchanged() {
    let cancel = CancellationToken::new();
    let (upstream, last_body) = spawn_mock_embeddings_upstream(cancel.clone(), None).await;
    let (base, proxy_cancel) = spawn_proxy(upstream, EMBED_MODEL, embedding_tags()).await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&json!({
            "model": EMBED_MODEL,
            "input": ["alpha", "beta", "gamma"],
            "encoding_format": "float",
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);

    let forwarded: Value = serde_json::from_slice(
        &last_body
            .lock()
            .await
            .clone()
            .expect("upstream saw a request"),
    )
    .unwrap();
    assert_eq!(forwarded["input"], json!(["alpha", "beta", "gamma"]));
    assert_eq!(
        forwarded["encoding_format"], "float",
        "fields the proxy does not model must survive the hop"
    );

    proxy_cancel.cancel();
    cancel.cancel();
}

/// A model without the `embedding` tag is refused before anything is loaded.
/// The upstream must see nothing at all — a request that reached it would mean
/// the proxy had already paid for a swap.
#[tokio::test]
async fn a_model_without_the_embedding_tag_is_refused_before_the_swap() {
    let cancel = CancellationToken::new();
    let (upstream, last_body) = spawn_mock_embeddings_upstream(cancel.clone(), None).await;
    let (base, proxy_cancel) = spawn_proxy(upstream, CHAT_MODEL, vec![]).await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&json!({ "model": CHAT_MODEL, "input": "hello" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "not_an_embedding_model");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("gglib model retag"),
        "the message must name the remedy: {}",
        body["error"]["message"]
    );

    assert!(
        last_body.lock().await.is_none(),
        "the upstream must never be reached for a model that cannot embed"
    );

    proxy_cancel.cancel();
    cancel.cancel();
}

/// Same status the chat path gives for an unknown model, so a client does not
/// have to learn two answers for one condition.
#[tokio::test]
async fn an_unknown_model_is_404() {
    let cancel = CancellationToken::new();
    let (upstream, _) = spawn_mock_embeddings_upstream(cancel.clone(), None).await;
    let (base, proxy_cancel) = spawn_proxy(upstream, EMBED_MODEL, embedding_tags()).await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&json!({ "model": "no-such-model", "input": "hello" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "model_not_found");

    proxy_cancel.cancel();
    cancel.cancel();
}

#[tokio::test]
async fn a_body_that_is_not_json_is_400() {
    let cancel = CancellationToken::new();
    let (upstream, _) = spawn_mock_embeddings_upstream(cancel.clone(), None).await;
    let (base, proxy_cancel) = spawn_proxy(upstream, EMBED_MODEL, embedding_tags()).await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .header("content-type", "application/json")
        .body("{ not json")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["error"]["code"], "invalid_request");

    proxy_cancel.cancel();
    cancel.cancel();
}

/// llama-server's own 501 — what a server started without `--embeddings`
/// returns — has to reach the client verbatim. It names the real cause better
/// than any status this layer could substitute for it.
#[tokio::test]
async fn an_upstream_error_passes_through_verbatim() {
    let cancel = CancellationToken::new();
    let (upstream, _) = spawn_mock_embeddings_upstream(
        cancel.clone(),
        Some((
            501,
            "This server does not support embeddings. Start it with `--embeddings`",
        )),
    )
    .await;
    let (base, proxy_cancel) = spawn_proxy(upstream, EMBED_MODEL, embedding_tags()).await;

    let resp = Client::new()
        .post(format!("{base}/v1/embeddings"))
        .json(&json!({ "model": EMBED_MODEL, "input": "hello" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 501);
    let body: Value = resp.json().await.unwrap();
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("--embeddings"),
        "upstream diagnosis must not be swallowed: {body}"
    );

    proxy_cancel.cancel();
    cancel.cancel();
}
