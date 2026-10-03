//! The one image policy: what an image costs, and who may be sent one.
//!
//! A chat message may carry images as `image_url` parts
//! ([`super::content::image_urls`] is the one walk over them). A model reads
//! them only when it is linked to a projector, and llama-server turns each
//! into prompt tokens by its pixel size, never by the length of the base64
//! that carries it. So two things are decided here, once, for every surface:
//!
//! - **The cost.** [`estimate_image_tokens`] from the size
//!   [`mod@super::image_size`] reads off the header, and [`image_url_tokens`] for
//!   a URL, which charges the cap when the size cannot be read.
//!   [`super::measure`] counts an image at this, so a screenshot is not
//!   refused as a megabyte of text.
//! - **The refusal.** [`refuse_unless_can_see`], which holds the error code
//!   and the remedy, so a request with an image for a model with no
//!   projector is refused by name before the model is loaded.

use serde_json::Value;

use super::content::image_urls;
use super::image_size::data_url_image_size;

/// The side, in pixels, of the square one image token covers.
///
/// From the 2026-10-03 reading in `docs/adr/log-0015.md`: a 2560x1440
/// screenshot was 3,646 prompt tokens measured, and 80 x 45 squares of 32 is
/// 3,600.
pub const IMAGE_TOKEN_PX: u32 = 32;

/// The most tokens one image is charged, and what an image whose size cannot
/// be read is charged.
///
/// The model family's own cap: the image is scaled down to fit it before it
/// is tokenized.
pub const MAX_IMAGE_TOKENS: usize = 4096;

/// The prompt tokens an image of `width` by `height` pixels is estimated to
/// take: one per [`IMAGE_TOKEN_PX`] square it touches, at least 1 and at most
/// [`MAX_IMAGE_TOKENS`].
#[must_use]
pub fn estimate_image_tokens(width: u32, height: u32) -> usize {
    let across = u64::from(width.div_ceil(IMAGE_TOKEN_PX));
    let down = u64::from(height.div_ceil(IMAGE_TOKEN_PX));
    usize::try_from(across * down)
        .map_or(MAX_IMAGE_TOKENS, |tokens| tokens.clamp(1, MAX_IMAGE_TOKENS))
}

/// The prompt tokens the image at `url` is charged.
///
/// Its estimate when `url` is a base64 data URL of a PNG or a JPEG whose
/// header can be read, and [`MAX_IMAGE_TOKENS`] otherwise (an `http(s)` URL,
/// another format, a payload cut short).
#[must_use]
pub fn image_url_tokens(url: &str) -> usize {
    data_url_image_size(url).map_or(MAX_IMAGE_TOKENS, |(width, height)| {
        estimate_image_tokens(width, height)
    })
}

/// Every image URL in the request's `messages`, history included, in order.
pub fn request_image_urls(body: &Value) -> impl Iterator<Item = &str> {
    body.get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|message| message.get("content"))
        .flat_map(image_urls)
}

/// Whether any message of the request carries an image.
#[must_use]
pub fn has_images(body: &Value) -> bool {
    request_image_urls(body).next().is_some()
}

/// The refusal of a request that carries an image for a model that cannot
/// read one. Every surface answers with [`Self::code`] and [`Self::message`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CannotReadImages;

impl CannotReadImages {
    /// The error code a client matches on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "model_cannot_read_images"
    }

    /// What the user is told: which model, why, and the command that fixes
    /// it.
    #[must_use]
    pub fn message(&self, model: &str) -> String {
        format!(
            "Model '{model}' cannot read images: it has no projector linked. Link one with \
             `gglib model update {model} --projector <path>`, or name a model that has one."
        )
    }
}

/// Refuse a request with images for a model with no image input.
///
/// `image_input` is the model's ([`crate::ports::ModelSummary::image_input`]);
/// `has_images` is [`has_images`] of the request, history included, since the
/// whole history is sent to the model every turn.
///
/// # Errors
///
/// [`CannotReadImages`] when the request has an image and the model cannot
/// read one.
pub const fn refuse_unless_can_see(
    image_input: bool,
    has_images: bool,
) -> Result<(), CannotReadImages> {
    if has_images && !image_input {
        Err(CannotReadImages)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod images_tests;
