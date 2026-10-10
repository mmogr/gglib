//! Images through `POST /v1/chat/completions`, over real HTTP.
//!
//! A model reads images when it is linked to a projector. A request with an
//! image for a model that is not is refused by name before admission: sent
//! on, it would load the model, evicting whatever was serving, to collect
//! llama-server's HTTP 500. A request for a model that is linked goes
//! through with its image as the client sent it, measured by the image's
//! tokens and not by its base64, and teaches the chars-per-token ratio
//! nothing.

mod fixtures;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use gglib_core::ports::{ModelCatalogPort, ModelRuntimePort};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use fixtures::common::{CountingRuntime, spawn_proxy_with_catalog};
use fixtures::images::{
    BLIND, DRAWS, SEES, Sight, Upstream, png_url, request, spawn_upstream, user_with_image,
};

/// A proxy over [`Sight`], its upstream, and the count of admissions.
struct Harness {
    base: String,
    upstream: Arc<Upstream>,
    admits: Arc<AtomicU64>,
    cancel: CancellationToken,
    proxy_cancel: CancellationToken,
}

impl Harness {
    async fn spawn() -> Self {
        let cancel = CancellationToken::new();
        let (port, upstream) = spawn_upstream(cancel.clone()).await;
        let (runtime, admits) = CountingRuntime::new(port, SEES);
        let (base, proxy_cancel) = spawn_proxy_with_catalog(
            runtime as Arc<dyn ModelRuntimePort>,
            Arc::new(Sight) as Arc<dyn ModelCatalogPort>,
        )
        .await;
        Self {
            base,
            upstream,
            admits,
            cancel,
            proxy_cancel,
        }
    }

    /// Send `body`, read the whole answer, and give its status and text.
    async fn post(&self, body: &Value) -> (StatusCode, String) {
        let response = Client::new()
            .post(format!("{}/v1/chat/completions", self.base))
            .json(body)
            .send()
            .await
            .unwrap();
        (response.status(), response.text().await.unwrap())
    }

    fn admits(&self) -> u64 {
        self.admits.load(Ordering::SeqCst)
    }

    fn stop(self) {
        self.proxy_cancel.cancel();
        self.cancel.cancel();
    }
}

fn error_of(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or_else(|e| panic!("not JSON ({e}): {text}"))
        ["error"]
        .clone()
}

#[tokio::test]
async fn an_image_for_a_model_with_no_projector_is_refused_by_name_before_admission() {
    let harness = Harness::spawn().await;
    let body = request(BLIND, &[user_with_image("what is this?", &png_url(400))]);

    let (status, text) = harness.post(&body).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    let error = error_of(&text);
    assert_eq!(error["code"], "model_cannot_read_images");
    assert_eq!(error["type"], "invalid_request_error");
    let message = error["message"].as_str().unwrap();
    assert!(message.contains("'blind' cannot read images"), "{message}");
    assert!(message.contains("no projector linked"), "{message}");
    assert!(
        message.contains("gglib model update blind --projector <path>"),
        "the message must name the command that fixes it: {message}"
    );
    assert_eq!(
        harness.admits(),
        0,
        "the refusal must run before the model swap, not after"
    );
    assert!(harness.upstream.seen.lock().unwrap().is_empty());
    harness.stop();
}

/// A model that draws is refused for chat by name before admission, with or
/// without an image in the request: its own refusal comes before the
/// projector's.
#[tokio::test]
async fn a_chat_for_an_image_model_is_refused_by_name_before_admission() {
    let harness = Harness::spawn().await;
    let text_only = request(DRAWS, &[json!({"role": "user", "content": "hi"})]);
    let with_image = request(DRAWS, &[user_with_image("draw this", &png_url(400))]);

    for body in [text_only, with_image] {
        let (status, text) = harness.post(&body).await;

        assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
        let error = error_of(&text);
        assert_eq!(error["code"], "image_model_cannot_chat", "{text}");
        assert_eq!(error["type"], "invalid_request_error");
        let message = error["message"].as_str().unwrap();
        assert!(message.contains("'draws' is an image model"), "{message}");
    }
    assert_eq!(
        harness.admits(),
        0,
        "the refusal must run before the model swap, not after"
    );
    assert!(harness.upstream.seen.lock().unwrap().is_empty());
    harness.stop();
}

/// `/v1/models` lists the image model as one that draws, with no context
/// window although its row records one.
#[tokio::test]
async fn the_list_names_an_image_model_and_gives_it_no_context_window() {
    let harness = Harness::spawn().await;

    let listed: Value = Client::new()
        .get(format!("{}/v1/models", harness.base))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let entry = listed["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == DRAWS)
        .unwrap_or_else(|| panic!("{DRAWS} is listed: {listed}"))
        .clone();
    assert_eq!(
        entry["capabilities"],
        json!(["image_generation"]),
        "{entry}"
    );
    assert!(entry.get("context_window").is_none(), "{entry}");
    harness.stop();
}

/// The whole history goes to the model every turn, so an image in an earlier
/// message is an image the model is asked to read.
#[tokio::test]
async fn an_image_in_the_history_is_refused_as_one_in_the_last_message_is() {
    let harness = Harness::spawn().await;
    let body = request(
        BLIND,
        &[
            user_with_image("what is this?", &png_url(400)),
            json!({"role": "assistant", "content": "A cat."}),
            json!({"role": "user", "content": "And what colour is it?"}),
        ],
    );

    let (status, text) = harness.post(&body).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert_eq!(error_of(&text)["code"], "model_cannot_read_images");
    assert_eq!(harness.admits(), 0);
    harness.stop();
}

#[tokio::test]
async fn a_model_with_no_projector_still_answers_text() {
    let harness = Harness::spawn().await;
    let body = request(BLIND, &[json!({"role": "user", "content": "image_url"})]);

    let (status, text) = harness.post(&body).await;

    assert_eq!(status, StatusCode::OK, "{text}");
    assert_eq!(harness.admits(), 1);
    harness.stop();
}

/// A 1.5 MB screenshot under this proxy's 4,096-token context: twenty-three
/// times the budget by its length, a fourteenth of it by its tokens.
#[tokio::test]
async fn a_linked_model_is_admitted_and_sent_the_image_as_the_client_sent_it() {
    let harness = Harness::spawn().await;
    let url = png_url(1_500_000);
    let body = request(SEES, &[user_with_image("what is the error?", &url)]);

    let (status, text) = harness.post(&body).await;

    assert_eq!(status, StatusCode::OK, "{text}");
    assert_eq!(harness.admits(), 1);
    let forwarded = harness.upstream.last();
    assert_eq!(forwarded["messages"], body["messages"]);
    assert_eq!(
        forwarded["messages"][0]["content"][1]["image_url"]["url"]
            .as_str()
            .map(str::len),
        Some(url.len())
    );
    harness.stop();
}

/// Two images whose size cannot be read are 8,192 tokens at the cap, and
/// the context is 4,096: the same answer any request over its budget gets.
#[tokio::test]
async fn images_over_the_context_are_context_length_exceeded() {
    let harness = Harness::spawn().await;
    let message = json!({"role": "user", "content": [
        {"type": "image_url", "image_url": {"url": "https://example.com/1.png"}},
        {"type": "image_url", "image_url": {"url": "https://example.com/2.png"}},
    ]});

    let (status, text) = harness.post(&request(SEES, &[message])).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert_eq!(error_of(&text)["code"], "context_length_exceeded");
    assert!(harness.upstream.seen.lock().unwrap().is_empty());
    harness.stop();
}

/// A text request 12,000 characters long: inside the 16,384 a 4,096-token
/// context is at the static 4 characters a token, and outside the 8,192 it is
/// at 2.
fn long_text_request(model: &str) -> Value {
    request(
        model,
        &[json!({"role": "user", "content": "x".repeat(12_000)})],
    )
}

/// The upstream reports a million prompt tokens for whatever it is sent,
/// which would teach a ratio at the floor of 2 characters a token and halve
/// the budget. An image request teaches nothing; the control below shows the
/// same report from a text request does.
#[tokio::test]
async fn an_image_request_does_not_move_the_chars_per_token_ratio() {
    let harness = Harness::spawn().await;
    harness
        .upstream
        .prompt_tokens
        .store(1_000_000, Ordering::SeqCst);
    let image = request(SEES, &[user_with_image("what is this?", &png_url(40_000))]);
    let (status, text) = harness.post(&image).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (status, text) = harness.post(&long_text_request(SEES)).await;

    assert_eq!(status, StatusCode::OK, "the budget is what it was: {text}");
    harness.stop();
}

#[tokio::test]
async fn a_text_request_still_moves_the_chars_per_token_ratio() {
    let harness = Harness::spawn().await;
    harness
        .upstream
        .prompt_tokens
        .store(1_000_000, Ordering::SeqCst);
    let short = request(SEES, &[json!({"role": "user", "content": "what is this?"})]);
    let (status, text) = harness.post(&short).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let (status, text) = harness.post(&long_text_request(SEES)).await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert_eq!(error_of(&text)["code"], "context_length_exceeded");
    harness.stop();
}
