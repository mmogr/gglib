//! `gglib run start`.
//!
//! `--image` sends each file inside the one message, as a `data:` URL: a
//! run's body is the whole request, and nothing is stored on this machine.
//! Each file is held to the rule `gglib q --image` holds one to
//! ([`checked_image`]: a PNG or a JPEG whose size can be read, at most
//! [`MAX_IMAGE_BYTES`]) and all of them together to
//! [`MAX_REQUEST_IMAGE_BYTES`], before anything is sent. Whether
//! the model can read an image is the proxy's to say, when the run starts.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use gglib_core::domain::attachment::AttachmentBlob;
use gglib_core::domain::runs::new_run_id;
use gglib_core::ports::attachment_store::AttachmentError;
use gglib_core::request_pipeline::{MAX_IMAGE_BYTES, MAX_REQUEST_IMAGE_BYTES};
use gglib_core::services::checked_image;
use serde_json::{Value, json};

use super::read;
use crate::daemon_client::DaemonHandle;

/// Start a run asking `model` one question, with the files at `images`
/// beside it. Prints the id on stdout alone, so `id=$(gglib run start …)`
/// works; with `follow` the id goes to stderr and the reply to stdout.
///
/// # Errors
///
/// The first file that cannot be sent, by its path, before any request.
pub(super) async fn start(
    daemon: &DaemonHandle,
    model: &str,
    prompt: &str,
    images: &[PathBuf],
    follow: bool,
) -> Result<()> {
    let id = new_run_id();
    let body = request(model, prompt, &read_images(images)?);
    let info = daemon.run_start(&id, &body).await?;
    if !follow {
        println!("{}", info.id);
        eprintln!("  Started. Read it with `gglib run show {}`.", info.id);
        return Ok(());
    }
    eprintln!("  run {}", info.id);
    read::print_reply(daemon, &info.id, true, 0).await
}

/// The chat request a run starts with: one user message, whose content is
/// the prompt alone, or with images the prompt and then one `image_url`
/// part an image, in the order given.
fn request(model: &str, prompt: &str, images: &[AttachmentBlob]) -> Value {
    let content = if images.is_empty() {
        json!(prompt)
    } else {
        let image = |image: &AttachmentBlob| json!({ "type": "image_url", "image_url": { "url": image.data_url() } });
        let text = json!({ "type": "text", "text": prompt });
        std::iter::once(text)
            .chain(images.iter().map(image))
            .collect()
    };
    json!({
        "model": model,
        "messages": [{ "role": "user", "content": content }],
    })
}

/// The files at `paths` as the images they are, in order.
fn read_images(paths: &[PathBuf]) -> Result<Vec<AttachmentBlob>> {
    let mut images = Vec::with_capacity(paths.len());
    let mut together = 0;
    for path in paths {
        let image = read_image(path)?;
        together += image.data.len();
        if together > MAX_REQUEST_IMAGE_BYTES {
            bail!(
                "image '{}': the images are more than {} MiB together, the most one request \
                 may carry. Send fewer or smaller images.",
                path.display(),
                MAX_REQUEST_IMAGE_BYTES >> 20
            );
        }
        images.push(image);
    }
    Ok(images)
}

/// The file at `path` as an image, held to core's one rule for an image.
/// The file's length is looked at first, so one over the cap is not read.
fn read_image(path: &Path) -> Result<AttachmentBlob> {
    let shown = path.display();
    let unreadable = |e: std::io::Error| anyhow!("cannot read image '{shown}': {e}");
    let len = std::fs::metadata(path).map_err(unreadable)?.len();
    if usize::try_from(len).map_or(true, |len| len > MAX_IMAGE_BYTES) {
        bail!("image '{shown}': {}", AttachmentError::TooLarge);
    }
    let data = std::fs::read(path).map_err(unreadable)?;
    let (mime, _) =
        checked_image(&data).map_err(|refused| anyhow!("image '{shown}': {refused}"))?;
    Ok(AttachmentBlob {
        mime: mime.to_owned(),
        data,
    })
}

#[cfg(test)]
#[path = "start_tests.rs"]
mod start_tests;
