//! Drawing through the daemon: [`DaemonImageGenerator`], the image
//! generation port over `POST /api/images/generations` with `stream: true`.
//!
//! The CLI never drives `sd-server` itself: the daemon owns the image model,
//! its turn on the GPU and its job, so a render started here queues behind
//! the daemon's chats and renders like any other. Each streamed progress
//! event becomes an [`ImageProgress`], each completed event an image, and an
//! error event or a refused request an [`ImageError::Refused`] carrying the
//! daemon's code and words.

use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::StreamExt as _;
use gglib_core::contracts::http::images::{ImageGenerationsRequest, ImageStreamEvent};
use gglib_core::domain::agent::PreviewFrame;
use gglib_core::ports::{
    GeneratedImage, ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest,
    ImageSize, ImageStage,
};
use gglib_core::request_pipeline::{PNG_MIME, image_mime, image_size};
use gglib_core::sse::DataFrames;
use serde_json::Value;
use tokio::sync::mpsc;

use super::{DaemonHandle, auth, paths};

/// The image generation port over a running daemon.
pub(crate) struct DaemonImageGenerator {
    daemon: DaemonHandle,
}

impl std::fmt::Debug for DaemonImageGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonImageGenerator")
            .finish_non_exhaustive()
    }
}

impl DaemonImageGenerator {
    /// Draw through `daemon`.
    pub(crate) const fn new(daemon: DaemonHandle) -> Self {
        Self { daemon }
    }
}

/// The body sent for `request`: streamed, no partial images.
pub(crate) fn body_of(request: &ImageRequest) -> ImageGenerationsRequest {
    ImageGenerationsRequest {
        model: request.model.clone(),
        prompt: request.prompt.clone(),
        n: Some(u32::from(request.n)),
        size: request.size.map(|s| s.to_string()),
        seed: request.seed,
        stream: true,
        ..ImageGenerationsRequest::default()
    }
}

#[async_trait]
impl ImageGenerationPort for DaemonImageGenerator {
    async fn generate(
        &self,
        request: ImageRequest,
        progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        let started = Instant::now();
        let unreachable = |e: &dyn std::fmt::Display| ImageError::Refused {
            status: 0,
            code: None,
            message: format!("could not reach the gglib daemon: {e}"),
        };
        let response = self
            .daemon
            .post(paths::IMAGES_GENERATIONS_PATH)
            .json(&body_of(&request))
            .send()
            .await
            .map_err(|e| unreachable(&e))?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(refusal(status, &body));
        }
        let rendered = read_render(response.bytes_stream(), &progress)
            .await
            .map_err(|e| unreachable(&e))??;
        Ok(ImageBatch {
            model: rendered
                .model
                .or(request.model)
                .unwrap_or_else(|| "the daemon's image model".to_owned()),
            images: rendered.images,
            elapsed: started.elapsed(),
        })
    }
}

/// The error a refused request's `body` names; for a 401, what the daemon
/// wanted and how to get it.
pub(crate) fn refusal(status: u16, body: &str) -> ImageError {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let error = parsed.as_ref().and_then(|v| v.get("error"));
    let field = |key: &str| {
        error
            .and_then(|e| e.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let message = if status == 401 {
        auth::unauthorized(body)
    } else {
        field("message")
            .or_else(|| error.and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| format!("the daemon answered {status}"))
    };
    ImageError::Refused {
        status,
        code: field("code"),
        message,
    }
}

/// What a render's stream brought: its images, and the image model that
/// drew them when the daemon named it.
#[derive(Debug)]
pub(crate) struct Rendered {
    pub(crate) images: Vec<GeneratedImage>,
    pub(crate) model: Option<String>,
}

/// Read a render's stream to its end, sending each progress event on and
/// keeping each completed image and the model it names; the images, or the
/// error event that ended it. The outer error is the transport's.
pub(crate) async fn read_render<B, E>(
    mut bytes: impl futures_util::Stream<Item = Result<B, E>> + Unpin,
    progress: &mpsc::Sender<ImageProgress>,
) -> anyhow::Result<Result<Rendered, ImageError>>
where
    B: AsRef<[u8]>,
    E: std::error::Error + Send + Sync + 'static,
{
    let mut frames = DataFrames::unbounded();
    let mut images = Vec::new();
    let mut drawn_by = None;
    while let Some(chunk) = bytes.next().await {
        let chunk = chunk.context("reading the render's events")?;
        for payload in frames.push(chunk.as_ref()) {
            let Ok(value) = serde_json::from_str::<Value>(&payload) else {
                continue;
            };
            if value.get("error").is_some() {
                return Ok(Err(refusal(500, &payload)));
            }
            match serde_json::from_value::<ImageStreamEvent>(value) {
                Ok(ImageStreamEvent::Completed {
                    b64_json,
                    size,
                    model,
                    ..
                }) => {
                    images.push(decoded(&b64_json, &size)?);
                    drawn_by = drawn_by.or(model);
                }
                Ok(event) => {
                    if let Some(report) = progress_of(&event) {
                        let _ = progress.try_send(report);
                    }
                }
                Err(e) => tracing::debug!("skipping an unreadable render event: {e}"),
            }
        }
    }
    if images.is_empty() {
        return Ok(Err(ImageError::Failed {
            message: "the daemon ended the render with no image".to_owned(),
        }));
    }
    Ok(Ok(Rendered {
        images,
        model: drawn_by,
    }))
}

fn decoded(b64: &str, size: &str) -> anyhow::Result<GeneratedImage> {
    let bytes = BASE64.decode(b64).context("an image that is not base64")?;
    let (width, height) = image_size(&bytes)
        .or_else(|| size.parse::<ImageSize>().ok().map(|s| (s.width, s.height)))
        .unwrap_or_default();
    Ok(GeneratedImage {
        mime: image_mime(&bytes).unwrap_or(PNG_MIME),
        bytes,
        width,
        height,
    })
}

/// The report a progress event carries; `None` for any other event.
pub(crate) fn progress_of(event: &ImageStreamEvent) -> Option<ImageProgress> {
    let ImageStreamEvent::Progress {
        stage,
        pass,
        step,
        total,
        position,
        behind,
        frame_b64,
    } = event
    else {
        return None;
    };
    let stage = match stage.as_str() {
        "queued" => ImageStage::Queued {
            position: position.unwrap_or(1),
            behind: behind.clone(),
        },
        "loading" => ImageStage::Loading,
        "sampling" => ImageStage::Sampling {
            pass: pass.unwrap_or(1),
            step: (*step)?,
            total: (*total)?,
        },
        "decoding" => ImageStage::Decoding,
        "finishing" => ImageStage::Finishing,
        _ => return None,
    };
    let preview = match (&stage, frame_b64) {
        (ImageStage::Sampling { step, total, .. }, Some(b64)) => {
            Some(PreviewFrame::png(*step, *total, b64.as_str()))
        }
        _ => None,
    };
    Some(ImageProgress { stage, preview })
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
