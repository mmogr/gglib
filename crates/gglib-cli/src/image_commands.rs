//! `gglib image`'s arguments.

use std::path::PathBuf;

use clap::Args;
use gglib_core::ports::{ImageSize, MAX_IMAGES_PER_REQUEST};

/// `gglib image "<prompt>"`: draw with this machine's image model, through
/// the daemon, and save what it drew.
#[derive(Args, Debug, Clone)]
pub struct ImageCommandArgs {
    /// What to draw
    pub prompt: String,
    /// The size, as `WIDTHxHEIGHT` (each side a multiple of 64, or 32 for
    /// Qwen-Image 2.1, from 256 to 1536) [default: 1024x1024]
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
    /// Where to save the image; `gglib-<unix time>.png` here when omitted.
    /// With more than one, each name gains -1, -2, …
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    pub output: Option<PathBuf>,
}
