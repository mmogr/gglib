//! The flag `chat` and `question` share for attaching images to a turn.

use std::path::PathBuf;

use clap::Args;

/// The image files a turn carries.
///
/// Long form only, and repeatable: one `--image` per file, sent in the
/// order given. Each file is stored once in this machine's database and
/// the message names it by id; the model it is sent to must have a
/// projector linked.
#[derive(Args, Debug, Clone, Default)]
pub struct ImageArgs {
    /// Attach a PNG or JPEG image to the first message sent; may be repeated
    #[arg(long = "image", value_name = "PATH")]
    pub images: Vec<PathBuf>,
}
