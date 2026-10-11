//! The drawing route's wire: what a client sends, reads, and is streamed.
//!
//! `POST /v1/images/generations` is the proxy's drawing route, shaped as
//! `OpenAI`'s, and `POST /api/images/generations` its twin at the daemon.
//!
//! The shapes follow `OpenAI`'s Images API: a body of `model`, `prompt`, `n`,
//! `size` (`WIDTHxHEIGHT`), `response_format` (only `b64_json` here),
//! `output_format` (only `png`), `stream` and `partial_images` (0 to 3), plus
//! gglib's `seed`; unknown fields are ignored. The streamed event names and
//! keys, `image_generation.partial_image` and `image_generation.completed`,
//! were checked against the openai-python and openai-node sources on
//! 2026-10-10 (`ImageGenPartialImageEvent`, `ImageGenCompletedEvent`). Both
//! SDKs read an event of an unknown `type` without raising: Python builds the
//! first variant leniently and Node does not validate. So gglib's own
//! `image_generation.progress` rides in the same stream, and an SDK user
//! who matches on `type` skips it. Both SDKs raise on a payload with an
//! `error` key, which is how a failure partway through a stream ends it.

use serde::{Deserialize, Serialize};

use crate::domain::{ComponentRole, ImageFamily};

/// The proxy's route.
pub const IMAGES_GENERATIONS_PATH: &str = "/v1/images/generations";

/// The proxy's answer to whether this machine can draw:
/// [`DrawingAvailability`].
pub const IMAGES_DRAWING_PATH: &str = "/v1/images/drawing";

/// The streamed event that reports a stage, a step or a place in line.
pub const PROGRESS_EVENT: &str = "image_generation.progress";

/// `OpenAI`'s streamed event for a partial image.
pub const PARTIAL_IMAGE_EVENT: &str = "image_generation.partial_image";

/// `OpenAI`'s streamed event for a finished image, one per image.
pub const COMPLETED_EVENT: &str = "image_generation.completed";

/// The most partial images a streamed request may ask for.
pub const MAX_PARTIAL_IMAGES: u32 = 3;

/// The body of a request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageGenerationsRequest {
    /// The image model, by id or name; absent for this machine's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// What to draw.
    pub prompt: String,
    /// How many images, 1 to 4; absent for one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// `WIDTHxHEIGHT`, or `auto`; absent for the family's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// gglib's own: the seed; absent for a random one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Only `b64_json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_format: Option<String>,
    /// Only `png`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    /// Stream the render's progress as server-sent events.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
    /// How many partial images a streamed render sends, 0 to 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<u32>,
}

/// One image of an answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageData {
    /// The PNG, base64.
    pub b64_json: String,
}

/// The answer to a request that did not stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageGenerationsResponse {
    /// Unix seconds.
    pub created: i64,
    /// The images, in the order drawn.
    pub data: Vec<ImageData>,
    /// Always `png`.
    pub output_format: String,
}

/// One event of a streamed render, named by its `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageStreamEvent {
    /// gglib's: where the render has got to.
    #[serde(rename = "image_generation.progress")]
    Progress {
        /// `queued`, `loading`, `sampling`, `decoding` or `finishing`.
        stage: String,
        /// The sampling pass, 1-based, while sampling.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pass: Option<u32>,
        /// The step finished, while sampling.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<u32>,
        /// The steps the pass takes, while sampling.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u32>,
        /// The place in line, 1 being next, while queued.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        position: Option<u32>,
        /// What is in the way, while queued, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        behind: Option<String>,
        /// The step's preview, a small PNG, base64.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        frame_b64: Option<String>,
    },
    /// `OpenAI`'s: a partial image, the preview at an evenly spread step.
    #[serde(rename = "image_generation.partial_image")]
    PartialImage {
        /// The preview, base64 PNG.
        b64_json: String,
        /// 0-based.
        partial_image_index: u32,
        /// The size asked for the finished image, as the request sent it
        /// (`auto` when it gave none), which is what `OpenAI` puts here.
        /// Never the size of this frame, which is a small preview, about
        /// 128 pixels a side.
        size: String,
        /// Always `png`.
        output_format: String,
        /// Unix seconds.
        created_at: i64,
    },
    /// `OpenAI`'s: one finished image.
    #[serde(rename = "image_generation.completed")]
    Completed {
        /// The image, base64 PNG.
        b64_json: String,
        /// Its size, `WIDTHxHEIGHT`, read from the image.
        size: String,
        /// Always `png`.
        output_format: String,
        /// Unix seconds.
        created_at: i64,
        /// gglib's own: the name of the image model that drew it. The SDKs
        /// ignore a key they do not know; absent from an older daemon.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
}

/// Whether a message sent with Draw pressed can draw here, and if not why.
///
/// What greys the Draw button, with its reason as the button's title. With
/// it, every image model on this machine, for a surface that lets a person
/// pick one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct DrawingAvailability {
    /// Whether it can.
    pub available: bool,
    /// Why not, as the code a request to draw would be refused with:
    /// `drawing_unavailable`, or for a chosen model `model_not_found`,
    /// `not_an_image_model` or `image_model_incomplete`; absent when it can.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Why not, in words that say what would fix it; absent when it can.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The image model that would draw, when it can: the one the question
    /// chose, else the one a request that names none draws with.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// That model's id, the one in [`ImageModelRow::id`]. Absent when it
    /// cannot draw, and from a service that names a model without its id.
    #[cfg_attr(feature = "ts-bindings", ts(optional = nullable))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<u32>,
    /// Every image model on this machine, complete or not, by family (in
    /// [`ImageFamily::ALL`]'s order) and then by name. Listed whenever the
    /// image driver could read them, also when `available` is false; empty
    /// when the question was refused before the driver was asked, or there
    /// is no image runtime.
    #[serde(default)]
    pub models: Vec<ImageModelRow>,
}

/// One image model on this machine, as a picker shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ImageModelRow {
    /// Its id in this machine's catalogue: what a client sends back to
    /// choose it.
    pub id: u32,
    /// Its name.
    pub name: String,
    /// Its family.
    pub family: ImageFamily,
    /// Whether it has a file for every role its family needs. One that does
    /// not cannot be chosen.
    pub complete: bool,
    /// The roles it has no file for, in the recipe's order; empty when
    /// `complete`.
    pub missing: Vec<ComponentRole>,
    /// Whether it is the one a request that names no model draws with: the
    /// settings' default, else the only complete image model. True on at
    /// most one row, and on none when such a request would be refused.
    pub default: bool,
    /// Whether it is loaded now. Choosing one that is not means a load
    /// before the picture starts.
    pub resident: bool,
}

impl DrawingAvailability {
    /// It can, with `model`.
    #[must_use]
    pub fn with(model: impl Into<String>) -> Self {
        Self {
            available: true,
            code: None,
            reason: None,
            model: Some(model.into()),
            model_id: None,
            models: Vec::new(),
        }
    }

    /// It cannot, for `reason`, coded `code`.
    #[must_use]
    pub fn refused(code: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            available: false,
            code: Some(code.into()),
            reason: Some(reason.into()),
            model: None,
            model_id: None,
            models: Vec::new(),
        }
    }

    /// The same answer over this machine's image models.
    #[must_use]
    pub fn listing(mut self, models: Vec<ImageModelRow>) -> Self {
        self.models = models;
        self
    }
}

/// The query of a drawing availability route.
///
/// `far` when the chat's model is on another machine, `calls_tools=false`
/// for a model that calls no tools, `image_model=<id>` to judge the image
/// model a person chose.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct DrawingQuery {
    /// The chat's model is on another machine.
    #[serde(default)]
    pub far: bool,
    /// Whether the chat's model calls tools; absent when unknown.
    #[serde(default)]
    pub calls_tools: Option<bool>,
    /// The id of the image model chosen ([`ImageModelRow::id`]), judged as a
    /// request naming it would be; absent to ask what a request that names
    /// none draws with.
    #[serde(default)]
    pub image_model: Option<u32>,
}

/// The steps, counted across every pass (`1..=total`), at which a render
/// asking for `partial_images` sends one: at most that many, evenly spread,
/// the last at the last step.
#[must_use]
pub fn partial_steps(total: u32, partial_images: u32) -> Vec<u32> {
    let wanted = partial_images.min(total);
    let mut steps: Vec<u32> = (1..=wanted).map(|i| (i * total).div_ceil(wanted)).collect();
    steps.dedup();
    steps
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
