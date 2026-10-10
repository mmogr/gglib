//! The streamed form of `POST /v1/images/generations`: server-sent events,
//! each named by its `type` and carrying it in its JSON.
//!
//! - `image_generation.progress` (gglib's) for every stage the driver reports:
//!   queued with its place and what is in the way, loading, sampling with its
//!   pass, step, total and preview frame, decoding, finishing.
//! - `image_generation.partial_image` (`OpenAI`'s) at most `partial_images`
//!   times, at the steps [`partial_steps`] spreads across every pass. Its
//!   `size` is the size asked for the finished image, as `OpenAI` has it,
//!   not the preview frame's own, which is about 128 pixels a side.
//! - `image_generation.completed` (`OpenAI`'s) once per image, last, with
//!   gglib's `model`: the name of the image model that drew it.
//!
//! A failure partway is one `error` event whose JSON is the error body a
//! response would carry; both `OpenAI` SDKs raise on it. The render runs inside
//! the stream, so a client that leaves drops it.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{Stream, StreamExt as _};
use gglib_core::contracts::http::images::{ImageStreamEvent, partial_steps};
use gglib_core::ports::{ImageGenerationPort, ImageProgress, ImageStage};
use tokio::sync::mpsc;

use crate::images::{Ask, refusal_body};

/// How often an idle stream says it is alive: a render can go 40 s before
/// its first step, and longer behind another render.
const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// Answer `ask` as a stream.
pub(crate) fn respond(images: Arc<dyn ImageGenerationPort>, ask: Ask) -> Response {
    let events = events(images, ask).map(Ok::<_, Infallible>);
    Sse::new(events)
        .keep_alive(KeepAlive::new().interval(KEEP_ALIVE))
        .into_response()
}

fn events(images: Arc<dyn ImageGenerationPort>, ask: Ask) -> impl Stream<Item = Event> {
    async_stream::stream! {
        let (reports, mut rx) = mpsc::channel(64);
        let mut partials = Partials::new(&ask);
        let work = images.generate(ask.request, reports);
        tokio::pin!(work);
        let result = loop {
            tokio::select! {
                biased;
                Some(progress) = rx.recv() => {
                    for event in partials.events(&progress) {
                        yield event;
                    }
                }
                result = &mut work => break result,
            }
        };
        while let Ok(progress) = rx.try_recv() {
            for event in partials.events(&progress) {
                yield event;
            }
        }
        match result {
            Ok(batch) => {
                for image in &batch.images {
                    yield sse(&ImageStreamEvent::Completed {
                        b64_json: BASE64.encode(&image.bytes),
                        size: format!("{}x{}", image.width, image.height),
                        output_format: "png".to_owned(),
                        created_at: now(),
                        model: Some(batch.model.clone()),
                    });
                }
            }
            Err(error) => {
                yield Event::default()
                    .event("error")
                    .data(serde_json::to_string(&refusal_body(&error)).unwrap_or_default());
            }
        }
    }
}

/// The partial images a render sends, and how many it has sent.
struct Partials {
    wanted: u32,
    passes: u32,
    size: String,
    sent: u32,
    /// Where they fall, once the first step says how many steps a pass takes.
    steps: Option<Vec<u32>>,
}

impl Partials {
    fn new(ask: &Ask) -> Self {
        Self {
            wanted: ask.partial_images,
            passes: u32::from(ask.request.n),
            size: ask.size_label.clone(),
            sent: 0,
            steps: None,
        }
    }

    /// The events one report becomes: its progress, then a partial image
    /// when its step is one of those chosen.
    fn events(&mut self, progress: &ImageProgress) -> Vec<Event> {
        let mut out = vec![sse(&progress_event(progress))];
        if let ImageStage::Sampling { pass, step, total } = progress.stage
            && let Some(frame) = &progress.preview
        {
            let steps = self
                .steps
                .get_or_insert_with(|| partial_steps(self.passes * total, self.wanted));
            let at = pass.saturating_sub(1) * total + step;
            if steps.contains(&at) {
                out.push(sse(&ImageStreamEvent::PartialImage {
                    b64_json: frame.b64.to_string(),
                    partial_image_index: self.sent,
                    size: self.size.clone(),
                    output_format: "png".to_owned(),
                    created_at: now(),
                }));
                self.sent += 1;
            }
        }
        out
    }
}

fn progress_event(progress: &ImageProgress) -> ImageStreamEvent {
    let frame_b64 = progress.preview.as_ref().map(|f| f.b64.to_string());
    let (stage, pass, step, total, position, behind) = match &progress.stage {
        ImageStage::Queued { position, behind } => {
            ("queued", None, None, None, Some(*position), behind.clone())
        }
        ImageStage::Loading => ("loading", None, None, None, None, None),
        ImageStage::Sampling { pass, step, total } => (
            "sampling",
            Some(*pass),
            Some(*step),
            Some(*total),
            None,
            None,
        ),
        ImageStage::Decoding => ("decoding", None, None, None, None, None),
        ImageStage::Finishing => ("finishing", None, None, None, None, None),
    };
    ImageStreamEvent::Progress {
        stage: stage.to_owned(),
        pass,
        step,
        total,
        position,
        behind,
        frame_b64,
    }
}

/// One event, named by its type.
fn sse(event: &ImageStreamEvent) -> Event {
    let name = match event {
        ImageStreamEvent::Progress { .. } => gglib_core::contracts::http::images::PROGRESS_EVENT,
        ImageStreamEvent::PartialImage { .. } => {
            gglib_core::contracts::http::images::PARTIAL_IMAGE_EVENT
        }
        ImageStreamEvent::Completed { .. } => gglib_core::contracts::http::images::COMPLETED_EVENT,
    };
    Event::default()
        .event(name)
        .data(serde_json::to_string(event).unwrap_or_default())
}

fn now() -> i64 {
    crate::images::unix_now()
}
