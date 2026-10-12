//! Drawing an image: [`ImageGenerationPort`], what it is asked
//! ([`ImageRequest`]), what it reports while it works ([`ImageProgress`]),
//! what it returns ([`ImageBatch`]) and why it refuses ([`ImageError`]).
//! A request may carry a picture to draw from ([`ImageSource`]): the whole
//! of it drawn again, or only where a mask is white.
//!
//! One driver implements it in the daemon (`gglib-runtime`'s `sd-server`
//! job driver), and the CLI implements it again over the daemon's
//! `/api/images/generations`, so the route, `gglib image` and the drawing
//! tool all ask the same question and read the same answer.
//!
//! A render takes minutes, so progress matters as much as the result: the
//! driver sends an [`ImageProgress`] for every stage it reaches and every
//! step it takes, and never waits on a slow reader to do it.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::contracts::http::images::DrawingAvailability;
use crate::domain::SizeRule;
use crate::domain::agent::PreviewFrame;
use crate::ports::{GateError, ModelRuntimeError};

/// The most images one request may ask for.
pub const MAX_IMAGES_PER_REQUEST: u8 = 4;

/// How long a render may take from submission, however it steps: the
/// driver's limit, and the drawing tool's deadline.
pub const IMAGE_JOB_DEADLINE: Duration = Duration::from_mins(30);

/// An image's size in pixels; written `WIDTHxHEIGHT` on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageSize {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl fmt::Display for ImageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.width, self.height)
    }
}

/// A size that is not `WIDTHxHEIGHT` with two whole numbers.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("\"{0}\" is not a size; write it as WIDTHxHEIGHT, such as 1024x1024")]
pub struct UnreadableSize(pub String);

impl FromStr for ImageSize {
    type Err = UnreadableSize;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let unreadable = || UnreadableSize(text.to_owned());
        let (width, height) = text.split_once(['x', 'X']).ok_or_else(unreadable)?;
        Ok(Self {
            width: width.trim().parse().map_err(|_| unreadable())?,
            height: height.trim().parse().map_err(|_| unreadable())?,
        })
    }
}

/// The strength a picture is redrawn at when a request gives one and names
/// no strength: most of the picture's shapes kept, its detail drawn again.
pub const DEFAULT_REDRAW_STRENGTH: f32 = 0.6;

/// A picture to draw from, in place of noise alone.
///
/// With no `mask` the whole picture is drawn again. With one, only where the
/// mask is white is, and every pixel of the result more than 16 pixels from
/// the white is the picture's own (the driver puts them back; the image
/// runtime alone moves them).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSource {
    /// The picture, an encoded PNG.
    pub image: Vec<u8>,
    /// The mask, an encoded PNG the picture's size: white where the picture
    /// is drawn again, black where it is kept.
    pub mask: Option<Vec<u8>>,
    /// How far from the picture the result may go: from 0.05, where almost
    /// nothing changes, to 1, where the picture is only a size.
    pub strength: f32,
}

/// What to draw.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRequest {
    /// The image model to draw with, by id or name; `None` for the settings'
    /// default image model, else the only one with every file.
    pub model: Option<String>,
    /// What to draw.
    pub prompt: String,
    /// The size; `None` for the family's default square.
    pub size: Option<ImageSize>,
    /// How many images, 1 to [`MAX_IMAGES_PER_REQUEST`].
    pub n: u8,
    /// The seed; `None` for a random one.
    pub seed: Option<i64>,
    /// The picture to draw from; `None` to draw from the prompt alone. With
    /// one, the result is the picture's size and `size` must be `None`.
    pub source: Option<ImageSource>,
}

impl ImageRequest {
    /// One image of `prompt`, every other choice left to the driver.
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            model: None,
            prompt: prompt.into(),
            size: None,
            n: 1,
            seed: None,
            source: None,
        }
    }
}

/// Where a render has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageStage {
    /// Waiting in line, `position` places from the front (1 being next),
    /// behind what `behind` names when the driver knows it.
    Queued {
        /// Place in line, 1 being next.
        position: u32,
        /// What is in the way, when known ("an image render").
        behind: Option<String>,
    },
    /// Loading the model and encoding the prompt: nothing reports a step
    /// until the first one finishes, about 40 s into a Flux render.
    Loading,
    /// Stepping: `step` of `total` in pass `pass` (one pass per image).
    Sampling {
        /// Which pass, 1-based.
        pass: u32,
        /// Steps done in this pass.
        step: u32,
        /// Steps this pass takes.
        total: u32,
    },
    /// The last step of the last pass is done; the pixels are being decoded.
    Decoding,
    /// The images are done and being handed back.
    Finishing,
}

/// One report from a render in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageProgress {
    /// Where it has got to.
    pub stage: ImageStage,
    /// The latest preview, sent with the step that made it.
    pub preview: Option<PreviewFrame>,
}

impl ImageProgress {
    /// A report of `stage` alone.
    #[must_use]
    pub const fn stage(stage: ImageStage) -> Self {
        Self {
            stage,
            preview: None,
        }
    }
}

/// One image drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedImage {
    /// The encoded image.
    pub bytes: Vec<u8>,
    /// Its media type; `"image/png"`.
    pub mime: &'static str,
    /// Width in pixels, read from the image itself.
    pub width: u32,
    /// Height in pixels, read from the image itself.
    pub height: u32,
}

/// What one request drew, with what a sentence about it needs: the model
/// that drew it and how long it took.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBatch {
    /// The image model's name.
    pub model: String,
    /// The images, in the order drawn.
    pub images: Vec<GeneratedImage>,
    /// From the request's arrival to the last image decoded.
    pub elapsed: Duration,
}

/// Why nothing was drawn. Each message says what to do next.
#[derive(Debug, Clone, Error)]
pub enum ImageError {
    /// The model named is not an image model.
    #[error(
        "'{model}' is not an image model, so it cannot draw; name an image model, or leave \
         the model out to draw with this machine's own"
    )]
    NotAnImageModel {
        /// The name asked for.
        model: String,
    },

    /// Nothing here can draw this request, for the reason given (no image
    /// model, several and none named, the only one missing a file).
    #[error("{reason}")]
    Unavailable {
        /// Why, and what would fix it.
        reason: String,
    },

    /// The size is not one the model's family draws.
    #[error(
        "{width}x{height} is not a size this model draws: {rule}; try {d}x{d}",
        d = rule.default_side
    )]
    InvalidSize {
        /// The width asked for.
        width: u32,
        /// The height asked for.
        height: u32,
        /// The family's rule.
        rule: SizeRule,
    },

    /// The request cannot be read as asked (no prompt, too many images).
    #[error("{message}")]
    Invalid {
        /// What is wrong and how to put it right.
        message: String,
    },

    /// The model could not be admitted or launched.
    #[error(transparent)]
    Runtime(ModelRuntimeError),

    /// No render turn was granted.
    #[error(transparent)]
    Gate(GateError),

    /// The image runtime took the job and failed it, or lost it.
    #[error(
        "the image runtime could not draw this: {message}; try another prompt or size, or retry"
    )]
    Failed {
        /// The runtime's own words.
        message: String,
    },

    /// A person stopped the image model while it drew this.
    #[error("the image model was stopped before the picture was finished")]
    Stopped,

    /// The render stopped stepping, and the image model was stopped.
    #[error(
        "the render made no progress for {}s, so the image model was stopped; retry",
        .after.as_secs()
    )]
    Stalled {
        /// How long it had gone without a step.
        after: Duration,
    },

    /// The render ran past its time limit, and the image model was stopped.
    #[error(
        "the render ran past its time limit, so the image model was stopped; try a smaller \
         size or fewer images"
    )]
    DeadlineExceeded,

    /// An image service elsewhere (the daemon, for the CLI) refused with this
    /// code and message.
    #[error("{message}")]
    Refused {
        /// The HTTP status it answered.
        status: u16,
        /// Its error code, when it gave one.
        code: Option<String>,
        /// Its message.
        message: String,
    },
}

impl ImageError {
    /// The error code a client matches on (`docs/error-codes.json`); `None`
    /// for a runtime error, whose code the proxy's own mapping of
    /// [`ModelRuntimeError`] gives, and for a refusal that carried none.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::NotAnImageModel { .. } => Some("not_an_image_model"),
            Self::Unavailable { .. } | Self::Gate(GateError::Unavailable(_)) => {
                Some("drawing_unavailable")
            }
            Self::InvalidSize { .. } => Some("invalid_image_size"),
            Self::Invalid { .. } => Some("invalid_request"),
            Self::Gate(GateError::Stalled(_)) => Some("admission_timeout"),
            Self::Failed { .. } | Self::Stopped => Some("image_generation_failed"),
            Self::Stalled { .. } | Self::DeadlineExceeded => Some("image_render_stalled"),
            Self::Refused { code, .. } => code.as_deref(),
            Self::Runtime(_) => None,
        }
    }

    /// The HTTP status a response carrying this error answers with.
    #[must_use]
    pub const fn http_status(&self) -> u16 {
        match self {
            Self::NotAnImageModel { .. }
            | Self::Unavailable { .. }
            | Self::InvalidSize { .. }
            | Self::Invalid { .. }
            | Self::Gate(GateError::Unavailable(_)) => 400,
            Self::Gate(GateError::Stalled(_)) => 503,
            Self::Failed { .. } | Self::Stopped => 502,
            Self::Stalled { .. } | Self::DeadlineExceeded => 504,
            Self::Runtime(e) => e.suggested_status_code(),
            Self::Refused { status, .. } => *status,
        }
    }
}

/// Draws images (see the [module docs](self)).
#[async_trait]
pub trait ImageGenerationPort: Send + Sync + fmt::Debug {
    /// Draw what `request` asks for, sending a report to `progress` at every
    /// stage and step. Sends never wait: a report the reader is not ready
    /// for is dropped.
    ///
    /// Dropping the returned future abandons the request and stops the GPU
    /// work it started: an implementation that cannot interrupt its runtime
    /// stops the runtime, and loads the model again for the next request.
    ///
    /// # Errors
    ///
    /// [`ImageError`], before anything queues for a request it can refuse
    /// from what it asks, and after for what the runtime answered.
    async fn generate(
        &self,
        request: ImageRequest,
        progress: mpsc::Sender<ImageProgress>,
    ) -> Result<ImageBatch, ImageError>;

    /// Whether this service can draw, with which image model, and every
    /// image model it has: the answer the Draw button reads. `chosen` is the
    /// id of the model a person picked, judged as a request naming it would
    /// be; `None` asks what a request that names none draws with. Asks
    /// without queueing or loading anything.
    ///
    /// The default answers that this service cannot say, so only a service
    /// that knows offers drawing.
    async fn drawing(&self, _chosen: Option<u32>) -> DrawingAvailability {
        DrawingAvailability::refused(
            "drawing_unavailable",
            "this image service cannot say whether it can draw",
        )
    }
}

#[cfg(test)]
#[path = "image_generation_tests.rs"]
mod tests;
