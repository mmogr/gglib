//! The images a CLI turn carries: `--image` on `q` and `chat`, and
//! `/image` in the REPL.
//!
//! Each file goes through core's one ingest
//! ([`AttachmentService::ingest`]) into this machine's database, in this
//! process, as it is on disk: the message then names it by id. A file that
//! is missing, is not a PNG or a JPEG, or is over the cap is an error that
//! names its path, before anything is asked of a model. What an image
//! costs is said once, on stderr, when it is attached. A session is then
//! refused as the daemon refuses a run ([`TurnImages::judge`]).

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use gglib_core::domain::agent::AgentMessage;
use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo, AttachmentUpload};
use gglib_core::ports::attachment_store::AttachmentError;
use gglib_core::request_pipeline::MAX_IMAGE_BYTES;
use gglib_core::services::AttachmentService;

use super::sight::Sight;

/// What `/image` with no path answers.
const USAGE: &str = "usage: /image <path>  (a PNG or a JPEG, attached to your next message)";

/// The images waiting for the next user message, and what attaches more.
pub(crate) struct TurnImages<'a> {
    service: &'a AttachmentService,
    sight: Sight,
    pending: Vec<AttachmentId>,
}

impl<'a> TurnImages<'a> {
    /// Store the files `--image` named, in order, for the first message.
    /// One receipt line a file goes to `receipts`, unless `quiet`.
    ///
    /// # Errors
    ///
    /// The first file that cannot be attached, by its path. Files before it
    /// stay stored, linked to nothing, until a daemon start a day later.
    pub(crate) async fn attach<W: io::Write>(
        service: &'a AttachmentService,
        paths: &[PathBuf],
        quiet: bool,
        receipts: &mut W,
    ) -> Result<Self> {
        let mut images = Self {
            service,
            sight: Sight::unjudged(),
            pending: Vec::new(),
        };
        for path in paths {
            let receipt = images.add(path).await?;
            if !quiet {
                writeln!(receipts, "{receipt}")?;
            }
        }
        Ok(images)
    }

    /// Take `sight` as what the session's model can read, and refuse the
    /// session as the daemon refuses a run, before its model is asked for
    /// anything: when an image it carries, in `history` or waiting for the
    /// first message, is not stored, or they are over the cap together
    /// ([`AttachmentService::check_request`]), and when the model cannot
    /// read one. The whole history is sent again each turn.
    ///
    /// # Errors
    ///
    /// Core's refusals, in its words.
    pub(crate) async fn judge(&mut self, sight: Sight, history: &[AgentMessage]) -> Result<()> {
        // The first message, as far as the checks read it: its images.
        let waiting = AgentMessage::User {
            content: String::new(),
            images: self.pending.clone(),
        };
        let carried: Vec<AgentMessage> = history
            .iter()
            .filter(|message| message.has_images())
            .cloned()
            .chain([waiting])
            .collect();
        self.service.check_request(&carried).await?;
        let has_images = carried.iter().any(AgentMessage::has_images);
        sight.admit(has_images).await?;
        self.sight = sight;
        Ok(())
    }

    /// The images for the message being sent; none wait after it.
    pub(crate) fn take(&mut self) -> Vec<AttachmentId> {
        std::mem::take(&mut self.pending)
    }

    /// Answer a REPL line that is the `/image` command: what to tell the
    /// user, be it the receipt, the usage or why the file was not attached.
    /// `None` for any other line.
    pub(crate) async fn command(&mut self, input: &str) -> Option<String> {
        let path = match image_command(input)? {
            ImageCommand::Attach(path) => path,
            ImageCommand::Usage => return Some(USAGE.to_owned()),
        };
        if let Err(refusal) = self.sight.admit(true).await {
            return Some(refusal.to_string());
        }
        Some(match self.add(&path).await {
            Ok(receipt) => receipt,
            Err(refused) => refused.to_string(),
        })
    }

    /// Store the file at `path` for the next message, and say what it is.
    async fn add(&mut self, path: &Path) -> Result<String> {
        let upload = ingest_file(self.service, path).await?;
        let receipt = receipt(path, &upload);
        self.pending.push(upload.info.id);
        Ok(receipt)
    }
}

/// What a REPL line that is the `/image` command asks for.
#[derive(Debug, PartialEq, Eq)]
enum ImageCommand {
    /// Attach the file at this path.
    Attach(PathBuf),
    /// `/image` alone: say how it is used.
    Usage,
}

/// `input` as the `/image` command, or `None` for any other line. The path
/// is the rest of the line, spaces included, without the quotes a terminal
/// may put around it.
fn image_command(input: &str) -> Option<ImageCommand> {
    let rest = input.strip_prefix("/image")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let path = rest.trim();
    let path = ['"', '\'']
        .iter()
        .find_map(|quote| path.strip_prefix(*quote)?.strip_suffix(*quote))
        .unwrap_or(path);
    Some(if path.is_empty() {
        ImageCommand::Usage
    } else {
        ImageCommand::Attach(PathBuf::from(path))
    })
}

/// The file at `path` through core's ingest, as it is on disk.
async fn ingest_file(service: &AttachmentService, path: &Path) -> Result<AttachmentUpload> {
    let shown = path.display();
    let unreadable = |e: std::io::Error| anyhow!("cannot read image '{shown}': {e}");
    let len = std::fs::metadata(path).map_err(unreadable)?.len();
    if usize::try_from(len).map_or(true, |len| len > MAX_IMAGE_BYTES) {
        bail!("image '{shown}': {}", AttachmentError::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(unreadable)?;
    service
        .ingest(&bytes)
        .await
        .map_err(|refused| anyhow!("image '{shown}': {refused}"))
}

/// The line that says what was attached and what it costs to send.
fn receipt(path: &Path, upload: &AttachmentUpload) -> String {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let AttachmentInfo { width, height, .. } = upload.info;
    format!(
        "  image {name}: {width}x{height}, ~{} tokens",
        upload.image_tokens
    )
}

/// How many characters of an id a marker shows, and the fewest
/// `gglib attachment save` takes.
pub(crate) const SHORT_ID_LEN: usize = 8;

/// The start of `id` a person reads and types: its first
/// [`SHORT_ID_LEN`] characters.
pub(crate) fn short_id(id: &AttachmentId) -> &str {
    &id.as_str()[..SHORT_ID_LEN]
}

/// The images of a message or a tool result as the transcript shows them:
/// ` [image WxH 3f9a2c1e]` for each, in order, with the start of its id for
/// `gglib attachment save`, and nothing for one with none.
pub(crate) fn markers(images: &[AttachmentInfo]) -> String {
    images.iter().fold(String::new(), |mut markers, image| {
        let (width, height, id) = (image.width, image.height, short_id(&image.id));
        let _ = write!(markers, " [image {width}x{height} {id}]");
        markers
    })
}

#[cfg(test)]
#[path = "images_check_tests.rs"]
mod images_check_tests;

#[cfg(test)]
#[path = "images_tests.rs"]
pub(crate) mod images_tests;

#[cfg(test)]
#[path = "image_command_tests.rs"]
mod image_command_tests;
