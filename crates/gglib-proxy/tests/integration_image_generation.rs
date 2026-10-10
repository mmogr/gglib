//! `POST /v1/images/generations` on the real proxy: inside the bearer guard
//! like every route a credential reaches, answered by the image driver the
//! daemon hands the proxy, and refused as unavailable on a proxy with none.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use gglib_core::ports::{
    GeneratedImage, ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest,
};
use gglib_core::{CorsConfig, DevicePorts, ProxyAccessConfig};
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::sync::mpsc;

mod fixtures;

/// Draws one three-byte image, whatever it is asked.
#[derive(Debug)]
struct OneImage;

#[async_trait]
impl ImageGenerationPort for OneImage {
    async fn generate(
        &self,
        _request: ImageRequest,
        _progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        Ok(ImageBatch {
            model: "sdxl".to_owned(),
            images: vec![GeneratedImage {
                bytes: vec![1, 2, 3],
                mime: "image/png",
                width: 1024,
                height: 1024,
            }],
            elapsed: Duration::from_secs(1),
        })
    }
}

async fn serve(
    images: Option<Arc<dyn ImageGenerationPort>>,
) -> (String, tokio_util::sync::CancellationToken) {
    let access = ProxyAccessConfig::new(
        CorsConfig::LocalOnly,
        Some("secret123".to_owned()),
        "127.0.0.1",
        vec![],
    )
    .with_devices(DevicePorts {
        images,
        ..DevicePorts::default()
    });
    let (base, _, cancel) = fixtures::access::spawn_proxy(access).await;
    (base, cancel)
}

#[tokio::test]
async fn the_route_needs_the_token_and_draws_with_the_daemons_driver() {
    let (base, cancel) = serve(Some(Arc::new(OneImage))).await;
    let url = format!("{base}/v1/images/generations");
    let client = reqwest::Client::new();
    let refused = client
        .post(&url)
        .json(&json!({"prompt": "a cat"}))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);

    let drawn = client
        .post(&url)
        .bearer_auth("secret123")
        .json(&json!({"prompt": "a cat"}))
        .send()
        .await
        .unwrap();
    assert_eq!(drawn.status(), StatusCode::OK);
    let body: Value = drawn.json().await.unwrap();
    assert_eq!(body["data"], json!([{"b64_json": "AQID"}]));
    cancel.cancel();
}

#[tokio::test]
async fn a_proxy_with_no_driver_cannot_draw() {
    let (base, cancel) = serve(None).await;
    let response = reqwest::Client::new()
        .post(format!("{base}/v1/images/generations"))
        .bearer_auth("secret123")
        .json(&json!({"prompt": "a cat"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "drawing_unavailable");
    cancel.cancel();
}
