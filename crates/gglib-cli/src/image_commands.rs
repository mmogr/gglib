//! `gglib image`'s arguments.

use std::path::PathBuf;

use clap::Args;
use gglib_core::ports::{ImageSize, MAX_IMAGES_PER_REQUEST};

/// `--strength`'s number: from 0.05 to 1, so not `nan` or `inf` either,
/// which would reach the daemon as no strength at all.
fn strength(text: &str) -> Result<f32, String> {
    text.parse::<f32>()
        .ok()
        .filter(|strength| (0.05..=1.0).contains(strength))
        .ok_or_else(|| format!("\"{text}\" is not a strength from 0.05 to 1"))
}

/// `gglib image "<prompt>"`: draw with this machine's image model, through
/// the daemon, and save what it drew. With `--from` it draws from a picture:
/// all of it again, or with `--mask` only where the mask is white.
#[derive(Args, Debug, Clone)]
pub struct ImageCommandArgs {
    /// What to draw
    pub prompt: String,
    /// The size, as `WIDTHxHEIGHT` (each side a multiple of 64, or 32 for
    /// Qwen-Image 2.1, or 16 for Z-Image, from 256 to 1536) [default:
    /// 1024x1024]
    #[arg(long, value_name = "WxH")]
    pub size: Option<ImageSize>,
    /// How many images, 1 to 4
    #[arg(
        short = 'n',
        long = "count",
        value_name = "N",
        default_value_t = 1,
        value_parser = clap::value_parser!(u8).range(1..=i64::from(MAX_IMAGES_PER_REQUEST))
    )]
    pub n: u8,
    /// The seed, for an image that can be drawn again [default: random]
    #[arg(long)]
    pub seed: Option<i64>,
    /// The image model to draw with, by name or id [default: the settings'
    /// default image model, else the only one with every file]
    #[arg(long, short)]
    pub model: Option<String>,
    /// A PNG to draw from, in place of starting from nothing: the result is
    /// its size, so `--size` is left out
    #[arg(long, value_name = "PNG", conflicts_with = "size")]
    pub from: Option<PathBuf>,
    /// A PNG the size of `--from`'s, white over what to draw again and
    /// black over what to keep; every pixel more than 16 pixels from the
    /// white is kept as it was
    #[arg(long, value_name = "PNG", requires = "from")]
    pub mask: Option<PathBuf>,
    /// How far from `--from`'s picture the result may go, from 0.05 (almost
    /// nothing changes) to 1 (only its size is kept) [default: 0.6]
    #[arg(long, value_name = "0.05-1", requires = "from", value_parser = strength)]
    pub strength: Option<f32>,
    /// Where to save the image; `gglib-<unix time>.png` here when omitted.
    /// With more than one, each name gains -1, -2, …
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    pub output: Option<PathBuf>,
}
