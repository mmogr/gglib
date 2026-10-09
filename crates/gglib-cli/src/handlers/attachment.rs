//! `gglib attachment save`: a stored image written to a file.
//!
//! A chat's transcript names each image by the start of its id
//! (` [image 1024x1024 3f9a2c1e]`, [`markers`](super::agent_chat::images::markers)).
//! This takes that start, or the whole id, finds the one image in this
//! machine's database it names, and writes its bytes as they were stored.

use std::fs::OpenOptions;
use std::io::{ErrorKind, Write as _};
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use gglib_core::domain::attachment::AttachmentId;
use gglib_core::services::AttachmentService;

use super::agent_chat::images::{SHORT_ID_LEN, short_id};
use crate::bootstrap::CliContext;

/// The longest id: a SHA-256 in hex.
const ID_LEN: usize = 64;

/// Run `gglib attachment save <id> [path]`, and say where it went.
///
/// # Errors
///
/// As [`save`].
pub(crate) async fn execute(
    ctx: &CliContext,
    id: &str,
    path: Option<&Path>,
    force: bool,
) -> Result<()> {
    let saved = save(ctx.app.attachments(), id, path, force).await?;
    println!("Saved {}", saved.display());
    Ok(())
}

/// Write the image `typed` names to `path`, or to `<id8>.<ext>` in the
/// working directory, or in `path` when it is a directory. An existing file
/// is replaced only when `force`.
///
/// # Errors
///
/// When `typed` names no image or more than one ([`resolve`]), the file
/// exists and `force` is not set, or the file cannot be written.
pub(crate) async fn save(
    service: &AttachmentService,
    typed: &str,
    path: Option<&Path>,
    force: bool,
) -> Result<PathBuf> {
    let id = resolve(service, typed).await?;
    let blob = service.blob(&id).await?;
    let name = format!("{}.{}", short_id(&id), extension(&blob.mime));
    let target = match path {
        None => PathBuf::from(name),
        Some(dir) if dir.is_dir() => dir.join(name),
        Some(file) => file.to_owned(),
    };
    write(&target, &blob.data, force)?;
    Ok(target)
}

/// The one stored image whose id starts with `typed`.
///
/// # Errors
///
/// When `typed` is not at least [`SHORT_ID_LEN`] hex characters, when no
/// stored image's id starts with it, and when more than one does.
pub(crate) async fn resolve(service: &AttachmentService, typed: &str) -> Result<AttachmentId> {
    let prefix = typed.trim().to_ascii_lowercase();
    let hex = prefix.bytes().all(|byte| byte.is_ascii_hexdigit());
    if !hex || !(SHORT_ID_LEN..=ID_LEN).contains(&prefix.len()) {
        bail!(
            "'{typed}' is not an image id: give at least {SHORT_ID_LEN} of its hex characters, \
             as a chat's [image WxH <id>] marker shows them."
        );
    }
    let mut ids = service.ids_starting_with(&prefix).await?;
    match ids.len() {
        0 => bail!("No stored image has an id starting {prefix}."),
        1 => Ok(ids.remove(0)),
        n => bail!("{n} stored images have ids starting {prefix}; give more of the id."),
    }
}

/// The file extension for an image's media type.
fn extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        _ => "img",
    }
}

/// Write `bytes` to `target`; an existing file is refused unless `force`.
fn write(target: &Path, bytes: &[u8], force: bool) -> Result<()> {
    let shown = target.display();
    let mut options = OpenOptions::new();
    options.write(true);
    if force {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut file = options.open(target).map_err(|e| match e.kind() {
        ErrorKind::AlreadyExists => {
            anyhow!("{shown} already exists; pass --force to replace it.")
        }
        _ => anyhow!("cannot write '{shown}': {e}"),
    })?;
    file.write_all(bytes)
        .with_context(|| format!("cannot write '{shown}'"))
}

#[cfg(test)]
#[path = "attachment_tests.rs"]
mod attachment_tests;
