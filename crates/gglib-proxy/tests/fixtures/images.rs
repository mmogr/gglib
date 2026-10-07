//! What the image tests need: a catalog with a model that reads images and
//! one that does not, an upstream that keeps what it is sent and reports the
//! prompt tokens it is told to, and a PNG as a data URL.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::extract::{DefaultBodyLimit, State};
use axum::{Router, body::Body, http::Response, routing::post};
use bytes::Bytes;
use gglib_core::ports::{CatalogError, ModelCatalogPort, ModelLaunchSpec, ModelSummary};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// The model linked to a projector.
pub(crate) const SEES: &str = "sees";
/// The model with none.
pub(crate) const BLIND: &str = "blind";

/// The base64 of a PNG's signature and `IHDR` for 640x480: 33 bytes, so 44
/// characters and no padding. 300 tokens by the estimate.
const PNG_640X480: &str = "iVBORw0KGgoAAAANSUhEUgAAAoAAAAHgCAYAAAAAAAAA";

/// A 640x480 PNG data URL `len` characters long, more or less: the header,
/// then zeros.
pub(crate) fn png_url(len: usize) -> String {
    format!(
        "data:image/png;base64,{PNG_640X480}{}",
        "AAAA".repeat(len / 4)
    )
}

/// A user message of `text` and one image.
pub(crate) fn user_with_image(text: &str, url: &str) -> Value {
    json!({"role": "user", "content": [
        {"type": "text", "text": text},
        {"type": "image_url", "image_url": {"url": url}},
    ]})
}

/// A streaming chat request for `model`.
pub(crate) fn request(model: &str, messages: &[Value]) -> Value {
    json!({"model": model, "stream": true, "messages": messages})
}

/// [`SEES`] and [`BLIND`], which differ in `image_input` alone.
#[derive(Debug)]
pub(crate) struct Sight;

impl Sight {
    fn models() -> Vec<ModelSummary> {
        vec![Self::model(1, SEES, true), Self::model(2, BLIND, false)]
    }

    fn model(id: u32, name: &str, image_input: bool) -> ModelSummary {
        ModelSummary {
            image_input,
            ..ModelSummary::bare(id, name)
        }
    }
}

#[async_trait]
impl ModelCatalogPort for Sight {
    async fn list_models(&self) -> Result<Vec<ModelSummary>, CatalogError> {
        Ok(Self::models())
    }

    async fn resolve_model(&self, name: &str) -> Result<Option<ModelSummary>, CatalogError> {
        Ok(Self::models().into_iter().find(|m| m.name == name))
    }

    async fn resolve_for_launch(
        &self,
        _name: &str,
    ) -> Result<Option<ModelLaunchSpec>, CatalogError> {
        Ok(None)
    }
}

/// What the upstream was sent, and what it reports.
#[derive(Debug, Default)]
pub(crate) struct Upstream {
    /// Every request body, in order.
    pub(crate) seen: Mutex<Vec<Bytes>>,
    /// The `usage.prompt_tokens` of every reply.
    pub(crate) prompt_tokens: AtomicU32,
}

impl Upstream {
    /// The last body it was sent, as JSON.
    pub(crate) fn last(&self) -> Value {
        let seen = self.seen.lock().unwrap();
        serde_json::from_slice(seen.last().expect("the upstream saw a request")).unwrap()
    }
}

/// An upstream that answers every completion with one word, a finish and a
/// usage frame, and takes a body of any size, as llama-server does.
pub(crate) async fn spawn_upstream(cancel: CancellationToken) -> (u16, Arc<Upstream>) {
    let upstream = Arc::new(Upstream::default());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().unwrap().port();

    let app = Router::new()
        .route(
            "/v1/chat/completions",
            post(|State(upstream): State<Arc<Upstream>>, body: Bytes| async move {
                upstream.seen.lock().unwrap().push(body);
                let prompt_tokens = upstream.prompt_tokens.load(Ordering::SeqCst);
                let usage = json!({"choices": [], "usage": {
                    "prompt_tokens": prompt_tokens,
                    "completion_tokens": 1,
                    "total_tokens": prompt_tokens + 1,
                }});
                Response::builder()
                    .header("content-type", "text/event-stream")
                    .header("cache-control", "no-cache")
                    .body(Body::from(format!(
                        "data: {{\"choices\":[{{\"delta\":{{\"content\":\"ok\"}},\"index\":0}}]}}\n\n\
                         data: {{\"choices\":[{{\"delta\":{{}},\"index\":0,\"finish_reason\":\"stop\"}}]}}\n\n\
                         data: {usage}\n\n\
                         data: [DONE]\n\n"
                    )))
                    .unwrap()
            }),
        )
        .layer(DefaultBodyLimit::disable())
        .with_state(Arc::clone(&upstream));

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { cancel.cancelled().await })
            .await
            .ok();
    });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    (port, upstream)
}
