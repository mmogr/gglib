//! Drawing through a daemon: [`ImageGenerator`], the image generation port
//! over `POST /api/images/generations` with `stream: true` on this machine's
//! daemon, or over the paired machine's `/v1/images/generations` through the
//! tunnel ([`FarProxy::generate_images`]). Both answer the same events.
//!
//! The CLI never drives `sd-server` itself: the daemon that draws owns the
//! image model, its turn on the GPU and its job, so a render started here
//! queues behind that daemon's chats and renders like any other. Each streamed progress
//! event becomes an [`ImageProgress`], each completed event an image, and an
//! error event or a refused request an [`ImageError::Refused`] carrying the
//! daemon's code and words.

use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::StreamExt as _;
use gglib_app_services::FarProxy;
use gglib_core::contracts::http::images::{
    DrawingAvailability, ImageGenerationsRequest, ImageStreamEvent,
};
use gglib_core::domain::agent::PreviewFrame;
use gglib_core::ports::{
    GeneratedImage, ImageBatch, ImageError, ImageGenerationPort, ImageProgress, ImageRequest,
    ImageSize, ImageStage,
};
use gglib_core::request_pipeline::{PNG_MIME, image_mime, image_size};
use gglib_core::services::drawing_refusal;
use gglib_core::sse::DataFrames;
use serde_json::Value;
use tokio::sync::mpsc;

use super::{DaemonHandle, auth, paths};

/// The image generation port over the daemon that draws.
pub(crate) enum ImageGenerator {
    /// This machine's daemon.
    Daemon(DaemonHandle),
    /// The paired machine's, through its proxy.
    Far(FarProxy),
}

impl std::fmt::Debug for ImageGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageGenerator").finish_non_exhaustive()
    }
}

impl ImageGenerator {
    /// The paired machine's name when it is the one drawing; `None` here.
    pub(crate) fn elsewhere(&self) -> Option<&str> {
        match self {
            Self::Daemon(_) => None,
            Self::Far(proxy) => Some(proxy.shown_name()),
        }
    }

    /// What a sentence about the daemon that draws calls it.
    fn who(&self) -> String {
        self.elsewhere()
            .map_or_else(|| "the gglib daemon".to_owned(), str::to_owned)
    }

    /// A render that never reached the daemon that draws, or whose stream
    /// broke off.
    fn unreachable(&self, e: &dyn std::fmt::Display) -> ImageError {
        ImageError::Refused {
            status: 0,
            code: None,
            message: format!("could not reach {}: {e}", self.who()),
        }
    }

    /// A refused render: the daemon's code and words, and for a key the
    /// paired machine no longer admits, what to do about it.
    fn refused(&self, status: u16, body: &str) -> ImageError {
        let mut refused = refusal(status, body);
        if let (Self::Far(proxy), 401, ImageError::Refused { message, .. }) =
            (self, status, &mut refused)
        {
            *message = proxy.key_refused();
        }
        refused
    }
}

/// The body sent for `request`: streamed, no partial images, and the
/// picture it draws from, when it has one, as base64.
pub(crate) fn body_of(request: &ImageRequest) -> ImageGenerationsRequest {
    let source = request.source.as_ref();
    ImageGenerationsRequest {
        model: request.model.clone(),
        prompt: request.prompt.clone(),
        n: Some(u32::from(request.n)),
        size: request.size.map(|s| s.to_string()),
        seed: request.seed,
        stream: true,
        image: source.map(|s| BASE64.encode(&s.image)),
        mask: source.and_then(|s| s.mask.as_ref().map(|mask| BASE64.encode(mask))),
        strength: source.map(|s| s.strength),
        ..ImageGenerationsRequest::default()
    }
}

#[async_trait]
impl ImageGenerationPort for ImageGenerator {
    async fn generate(
        &self,
        request: ImageRequest,
        progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError> {
        let started = Instant::now();
        let body = body_of(&request);
        let response = match self {
            Self::Daemon(daemon) => daemon
                .post(paths::IMAGES_GENERATIONS_PATH)
                .json(&body)
                .send()
                .await
                .map_err(|e| self.unreachable(&e))?,
            Self::Far(proxy) => proxy
                .generate_images(&body)
                .await
                .map_err(|e| self.unreachable(&e))?,
        };
        let status = response.status().as_u16();
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(self.refused(status, &body));
        }
        let rendered = read_render(response.bytes_stream(), &progress)
            .await
            .map_err(|e| self.unreachable(&e))??;
        Ok(ImageBatch {
            model: rendered
                .model
                .or(request.model)
                .unwrap_or_else(|| "the daemon's image model".to_owned()),
            images: rendered.images,
            elapsed: started.elapsed(),
        })
    }

    /// The drawing daemon's own answer (`GET /api/images/drawing` here,
    /// `/v1/images/drawing` there), as it gave it: its image model when it
    /// can draw, its code and reason when it cannot, and its image models;
    /// `chosen` goes as `?image_model=<id>`. A daemon that cannot be
    /// reached, or is too old to have the route, cannot draw.
    async fn drawing(&self, chosen: Option<u32>) -> DrawingAvailability {
        let unavailable = |reason: String| drawing_refusal(&ImageError::Unavailable { reason });
        let answered = match self {
            Self::Daemon(daemon) => {
                let asked = chosen.map_or_else(String::new, |id| format!("?image_model={id}"));
                let path = format!("{}{asked}", paths::IMAGES_DRAWING_PATH);
                daemon.get(&path).send().await.map_err(|e| e.to_string())
            }
            Self::Far(proxy) => proxy.drawing(chosen).await.map_err(|e| e.to_string()),
        };
        let response = match answered {
            Ok(response) => response,
            Err(e) => return unavailable(format!("could not reach {}: {e}", self.who())),
        };
        if !response.status().is_success() {
            let remedy = match self {
                Self::Daemon(_) => "restart it so it runs this version",
                Self::Far(_) => "it may run a gglib older than this one",
            };
            return unavailable(format!(
                "{} answered {} when asked whether it can draw; {remedy}",
                self.who(),
                response.status().as_u16()
            ));
        }
        response.json().await.unwrap_or_else(|e| {
            unavailable(format!("{}'s answer could not be read: {e}", self.who()))
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
