//! The picture a request draws from: read before anything queues, sent to
//! `sd-server` as it reads one, and put back around what a mask left alone.
//!
//! `sd-server` draws from `init_image`, and with `mask_image` draws again
//! only where the mask is white. It does not hand the rest back as it was:
//! the picture goes through the VAE both ways, and in the one render
//! measured 98.8% of the pixels outside the mask came back changed
//! (log-0018, 2026-10-12). So a masked render's result is pasted over the
//! picture here ([`Source::paste`]): the result where the mask is white, the
//! picture's own bytes more than [`FEATHER`] pixels from it, and a blend of
//! the two across the pixels between, so the patch has no hard edge.
//!
//! The picture's alpha is dropped and the mask made black or white before
//! they are sent: a canvas exports RGBA, `sd-server` reads a mask of one
//! channel, and a family that reads alpha (Qwen-Image 2.1) would take a
//! picture's as a second mask. A mask's own alpha is not read: white and
//! black are its colours. The previews of a masked render are `sd-server`'s
//! own frames, with nothing pasted.

use std::io::Cursor;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use gglib_core::ports::{ImageError, ImageSource};
use gglib_core::request_pipeline::image_size;
use image::{GrayImage, ImageFormat, Luma, RgbImage};

/// How far outside the mask's white a pasted result fades into the picture,
/// in pixels, counted along a row or a column, whichever is further: beyond
/// it every pixel is the picture's own. Even, so that it halves.
pub(crate) const FEATHER: u32 = 16;
const _: () = assert!(FEATHER.is_multiple_of(2));

/// Half of [`FEATHER`]: how far the mask's white is grown, and the reach of
/// the square the grown mask is then averaged over.
const RADIUS: u32 = FEATHER / 2;

/// The weakest and the strongest a redraw may be asked for. Below the first
/// the steps asked of `sd-server` grow past twenty times the recipe's.
pub(crate) const STRENGTH: std::ops::RangeInclusive<f32> = 0.05..=1.0;

/// A request's picture, decoded and judged.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Source {
    /// The picture, without alpha.
    picture: RgbImage,
    /// White (255) where the picture is drawn again, black (0) elsewhere;
    /// `None` to draw all of it again.
    mask: Option<GrayImage>,
    /// How much of a result each pixel takes ([`weights`]), worked out once
    /// for every image of the request; empty with no mask.
    weights: Vec<u32>,
    /// How far from the picture the result may go.
    pub(crate) strength: f32,
}

fn invalid(message: impl Into<String>) -> ImageError {
    ImageError::Invalid {
        message: message.into(),
    }
}

/// The width and height `bytes`' header gives, read without decoding it;
/// `what` ("picture", "mask") names it in the refusal of bytes with none.
/// What is not a PNG after all is refused when it is decoded.
pub(crate) fn header_size(bytes: &[u8], what: &str) -> Result<(u32, u32), ImageError> {
    image_size(bytes)
        .ok_or_else(|| invalid(format!("the {what} is not a PNG whose size can be read")))
}

fn png(bytes: &[u8], what: &str) -> Result<image::DynamicImage, ImageError> {
    image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|e| invalid(format!("the {what} is not a PNG that can be read: {e}")))
}

fn encode(image: &image::DynamicImage) -> Result<Vec<u8>, ImageError> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .map_err(|e| ImageError::Failed {
            message: format!("a PNG could not be written: {e}"),
        })?;
    Ok(bytes)
}

impl Source {
    /// Decode `source` and refuse what no render could use: a picture or a
    /// mask that is not a PNG, a mask of another size (read from the two
    /// headers, before either is decoded), a mask with no white, a strength
    /// outside [`STRENGTH`].
    pub(crate) fn read(source: &ImageSource) -> Result<Self, ImageError> {
        if !STRENGTH.contains(&source.strength) {
            return Err(invalid(format!(
                "strength is from {} to {}, not {}",
                STRENGTH.start(),
                STRENGTH.end(),
                source.strength
            )));
        }
        let (width, height) = header_size(&source.image, "picture")?;
        if let Some(mask) = &source.mask {
            let (mask_width, mask_height) = header_size(mask, "mask")?;
            if (mask_width, mask_height) != (width, height) {
                return Err(invalid(format!(
                    "the mask is {mask_width}x{mask_height} and the picture {width}x{height}; \
                     a mask is the picture's size"
                )));
            }
        }
        let picture = png(&source.image, "picture")?.to_rgb8();
        let mask = match &source.mask {
            None => None,
            Some(bytes) => {
                let mut mask = png(bytes, "mask")?.to_luma8();
                for pixel in mask.pixels_mut() {
                    *pixel = Luma([if pixel.0[0] >= 128 { 255 } else { 0 }]);
                }
                if mask.pixels().all(|p| p.0[0] == 0) {
                    return Err(invalid(
                        "the mask has no white in it, so nothing would be drawn again; paint \
                         white over what to redraw",
                    ));
                }
                Some(mask)
            }
        };
        Ok(Self {
            weights: mask.as_ref().map(weights).unwrap_or_default(),
            picture,
            mask,
            strength: source.strength,
        })
    }

    /// The picture and the mask as `sd-server` reads them: base64 PNGs, the
    /// picture RGB and the mask one channel.
    pub(crate) fn wire(&self) -> Result<(String, Option<String>), ImageError> {
        let picture = encode(&image::DynamicImage::ImageRgb8(self.picture.clone()))?;
        let mask = self
            .mask
            .as_ref()
            .map(|mask| encode(&image::DynamicImage::ImageLuma8(mask.clone())))
            .transpose()?;
        Ok((BASE64.encode(picture), mask.map(|m| BASE64.encode(m))))
    }

    /// What the request is answered with for `result`, a PNG `sd-server`
    /// drew from this source: `result` itself when all of the picture was
    /// drawn again, and with a mask the picture with the result pasted in
    /// (see the [module docs](self)).
    ///
    /// # Errors
    ///
    /// [`ImageError::Failed`] for a result whose header is not the
    /// picture's size, and for a masked one that is not a PNG.
    pub(crate) fn paste(&self, result: Vec<u8>) -> Result<Vec<u8>, ImageError> {
        let failed = |message: String| ImageError::Failed { message };
        let (width, height) = self.picture.dimensions();
        if image_size(&result) != Some((width, height)) {
            return Err(failed(format!(
                "sd-server returned a picture that is not {width}x{height}, the size of the \
                 one it drew from"
            )));
        }
        if self.mask.is_none() {
            return Ok(result);
        }
        let drawn = image::load_from_memory_with_format(&result, ImageFormat::Png)
            .map_err(|e| {
                failed(format!(
                    "sd-server returned a picture that cannot be read: {e}"
                ))
            })?
            .to_rgb8();
        let mut pasted = self.picture.clone();
        for ((kept, new), weight) in pasted.pixels_mut().zip(drawn.pixels()).zip(&self.weights) {
            for (old, new) in kept.0.iter_mut().zip(new.0) {
                *old = blend(*old, new, *weight);
            }
        }
        encode(&image::DynamicImage::ImageRgb8(pasted))
    }
}

/// The most a weight is, all of the result: the pixels of the square
/// [`weights`] averages over.
const FULL: u32 = (2 * RADIUS + 1) * (2 * RADIUS + 1);

/// `old` and `new` mixed, `weight` of [`FULL`] parts `new`: exactly `old` at
/// 0 and exactly `new` at [`FULL`].
fn blend(old: u8, new: u8, weight: u32) -> u8 {
    let mixed = (u32::from(new) * weight + u32::from(old) * (FULL - weight) + FULL / 2) / FULL;
    u8::try_from(mixed).unwrap_or(u8::MAX)
}

/// How much of the result each pixel takes, of [`FULL`]: the mask's white
/// grown by half of [`FEATHER`], then averaged over a square that wide
/// again, so the weight is [`FULL`] on the white, 0 more than [`FEATHER`]
/// pixels from it, and falls between the two.
fn weights(mask: &GrayImage) -> Vec<u32> {
    let (width, height) = (mask.width() as usize, mask.height() as usize);
    let white: Vec<u32> = mask.pixels().map(|p| u32::from(p.0[0] != 0)).collect();
    let radius = RADIUS as usize;
    let near = window_sums(&white, width, height, radius);
    let grown: Vec<u32> = near.iter().map(|&count| u32::from(count > 0)).collect();
    window_sums(&grown, width, height, radius)
}

/// For each pixel, the sum of `values` over the square of `radius` around
/// it, a pixel off the edge counting as the nearest one on it.
fn window_sums(values: &[u32], width: usize, height: usize, radius: usize) -> Vec<u32> {
    let clamp =
        |at: usize, offset: usize, len: usize| (at + offset).saturating_sub(radius).min(len - 1);
    let mut rows = vec![0; values.len()];
    for y in 0..height {
        for x in 0..width {
            rows[y * width + x] = (0..=2 * radius)
                .map(|offset| values[y * width + clamp(x, offset, width)])
                .sum();
        }
    }
    let mut sums = vec![0; values.len()];
    for y in 0..height {
        for x in 0..width {
            sums[y * width + x] = (0..=2 * radius)
                .map(|offset| rows[clamp(y, offset, height) * width + x])
                .sum();
        }
    }
    sums
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
