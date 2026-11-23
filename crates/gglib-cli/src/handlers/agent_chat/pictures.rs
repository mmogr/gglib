//! A chat's pictures in the terminal.
//!
//! A chat names each picture it carries by a marker, ` [image WxH 3f9a2c1e]`
//! ([`markers`](super::images::markers)). On a terminal that opens links the
//! marker is one: a click opens the picture. Pictures are rows in this
//! machine's database, so a link needs a file to point at. The picture is
//! copied, as stored, into a folder beside the database that is this user's
//! alone, when its marker is printed. A copy not linked for [`KEPT`] is
//! removed when a session whose markers are links starts; `gglib attachment
//! save` is still how a picture is kept.
//!
//! On a terminal that draws pictures, one a tool made is also drawn under
//! the tool's line ([`Inline`]).
//!
//! Nothing here fails a turn: a picture that cannot be read or copied keeps
//! its plain marker, and one that cannot be drawn is not.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::Result;
use gglib_core::domain::attachment::{AttachmentId, AttachmentInfo};
use gglib_core::paths::{create_private_dir, create_private_file, database_path};
use gglib_core::services::AttachmentService;

use super::images::short_id;
use super::inline_picture::{Inline, Picture, at_least, columns};
use crate::handlers::attachment::extension;

/// How long a picture's copy is kept after its marker was last printed: a
/// week, longer than a terminal's scrollback is usually read back.
const KEPT: Duration = Duration::from_hours(7 * 24);

/// The longest address a link carries: iTerm2 and VTE read no more.
const LONGEST_ADDRESS: usize = 2083;

/// The links of some markers: each image's id, and the address of its copy.
#[derive(Debug, Default)]
pub(crate) struct Links(HashMap<AttachmentId, String>);

impl Links {
    /// The address `id`'s marker links to, when it has one.
    pub(crate) fn of(&self, id: &AttachmentId) -> Option<&str> {
        self.0.get(id).map(String::as_str)
    }
}

/// `text` as a link to `address` (OSC 8), for a terminal that opens them.
/// One that does not shows `text` alone.
pub(crate) fn link(address: &str, text: &str) -> String {
    format!("\x1b]8;;{address}\x1b\\{text}\x1b]8;;\x1b\\")
}

/// How a session shows its chat's pictures.
pub(crate) struct Pictures<'a> {
    service: &'a AttachmentService,
    /// Where the copies go; none when markers are not links here.
    folder: Option<PathBuf>,
    /// How this terminal draws a picture, when it does.
    inline: Option<Inline>,
}

impl<'a> Pictures<'a> {
    /// For markers written to a stream that is a terminal, or is not
    /// (`to_terminal`): links when it is one that opens them, pictures
    /// drawn when it is one that draws them, and plain markers otherwise.
    /// Copies past [`KEPT`] are removed.
    pub(crate) fn for_stream(service: &'a AttachmentService, to_terminal: bool) -> Self {
        let env = |name: &str| std::env::var(name).ok();
        let links = to_terminal && opens_links(env);
        let folder = links.then(folder).flatten();
        if let Some(folder) = &folder {
            sweep(folder, SystemTime::now());
        }
        let inline = to_terminal.then(|| Inline::of_terminal(env)).flatten();
        Self {
            service,
            folder,
            inline,
        }
    }

    /// With the copies in `folder`, whatever the terminal.
    #[cfg(test)]
    pub(crate) fn in_folder(service: &'a AttachmentService, folder: &Path) -> Self {
        Self {
            service,
            folder: Some(folder.to_owned()),
            inline: None,
        }
    }

    /// And drawing pictures as `inline` does.
    #[cfg(test)]
    pub(crate) fn drawing(mut self, inline: Inline) -> Self {
        self.inline = Some(inline);
        self
    }

    /// Plain markers.
    #[cfg(test)]
    pub(crate) fn plain(service: &'a AttachmentService) -> Self {
        Self {
            service,
            folder: None,
            inline: None,
        }
    }

    /// A link for each of `images` whose copy is in the folder, written now
    /// if it was not. One that cannot be copied has none.
    pub(crate) async fn links(&self, images: &[AttachmentInfo]) -> Links {
        let mut links = Links::default();
        let Some(folder) = &self.folder else {
            return links;
        };
        for image in images {
            match self.copy(folder, image).await {
                Ok(path) => {
                    if let Some(address) = address(&path) {
                        links.0.insert(image.id.clone(), address);
                    }
                }
                Err(e) => tracing::debug!("picture {} was not copied: {e}", image.id.as_str()),
            }
        }
        links
    }

    /// What to write under a tool's line to draw each of `images` this
    /// terminal can: nothing where it draws none. Each is drawn two columns
    /// in, as the line above it starts, and sized to the terminal's window.
    pub(crate) async fn drawn(&self, images: &[AttachmentInfo]) -> String {
        let window = crossterm::terminal::size().ok().map(|(columns, _)| columns);
        self.drawn_in(images, window).await
    }

    /// [`Self::drawn`], for a window `window` columns wide when that is
    /// known.
    async fn drawn_in(&self, images: &[AttachmentInfo], window: Option<u16>) -> String {
        let mut drawn = String::new();
        let Some(inline) = self.inline else {
            return drawn;
        };
        for image in images {
            let Ok(blob) = self.service.blob(&image.id).await else {
                continue;
            };
            let name = format!("{}.{}", short_id(&image.id), extension(&blob.mime));
            let picture = Picture {
                name: &name,
                mime: &blob.mime,
                bytes: &blob.data,
            };
            let columns = columns(image.width, image.height, window);
            if let Some(sequence) = inline.drawn(&picture, columns) {
                drawn.push_str("  ");
                drawn.push_str(&sequence);
            }
        }
        drawn
    }

    /// `image`'s copy in `folder`, under its whole id: the one already
    /// there, kept for another [`KEPT`], or its stored bytes written now.
    async fn copy(&self, folder: &Path, image: &AttachmentInfo) -> Result<PathBuf> {
        let name = format!("{}.{}", image.id.as_str(), extension(&image.mime));
        let path = folder.join(&name);
        if path.is_file() {
            if let Ok(file) = fs::File::options().write(true).open(&path) {
                let _ = file.set_modified(SystemTime::now());
            }
            return Ok(path);
        }
        let blob = self.service.blob(&image.id).await?;
        // Whole or not there: written beside it, then renamed.
        let part = folder.join(format!("{name}.{}.part", std::process::id()));
        let written = create_private_file(&part)
            .and_then(|()| fs::write(&part, &blob.data))
            .and_then(|()| fs::rename(&part, &path));
        if let Err(e) = written {
            let _ = fs::remove_file(&part);
            return Err(e.into());
        }
        Ok(path)
    }
}

/// The folder the copies go to, beside the database and this user's alone.
fn folder() -> Option<PathBuf> {
    let folder = database_path().ok()?.parent()?.join("pictures");
    create_private_dir(&folder).ok()?;
    Some(folder)
}

/// Remove what in `folder` was last linked more than [`KEPT`] before `now`.
fn sweep(folder: &Path, now: SystemTime) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|facts| facts.modified())
            .is_ok_and(|at| now.duration_since(at).is_ok_and(|age| age > KEPT));
        if old {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// `path` as the address a link carries: a `file://` URL, percent-encoded.
/// None for a path that is not absolute, or whose address is too long.
fn address(path: &Path) -> Option<String> {
    let address = String::from(reqwest::Url::from_file_path(path).ok()?);
    let plain = address.bytes().all(|byte| (32..=126).contains(&byte));
    (plain && address.len() <= LONGEST_ADDRESS).then_some(address)
}

/// Whether a link to a file here opens it, by the terminal's own
/// environment (`env`): there is no way to ask a terminal.
///
/// Not over SSH, where the file is on this machine and the click on another.
/// Not inside screen, nor in Emacs, whose terminal prints the sequence. Not
/// on a `dumb` terminal or the Linux console, which prints it too. With no
/// `TERM` at all, as on Windows, only in Windows Terminal.
///
/// Inside tmux from 3.4, told by the `TERM_PROGRAM` and
/// `TERM_PROGRAM_VERSION` tmux sets in its panes: it keeps a link itself and
/// passes it on to a terminal it knows opens them (its `hyperlinks`
/// feature), and shows the text alone to one it does not. An older tmux
/// drops the link, so it is not written there.
fn opens_links(env: impl Fn(&str) -> Option<String>) -> bool {
    let set = |name: &str| env(name).is_some_and(|value| !value.is_empty());
    if ["SSH_CONNECTION", "SSH_TTY", "STY", "INSIDE_EMACS"]
        .iter()
        .any(|name| set(name))
    {
        return false;
    }
    let term = env("TERM").unwrap_or_default();
    if ["dumb", "linux"].contains(&term.as_str()) {
        return false;
    }
    if set("TMUX") {
        return env("TERM_PROGRAM").as_deref() == Some("tmux")
            && env("TERM_PROGRAM_VERSION").is_some_and(|version| at_least(&version, (3, 4)));
    }
    match term.as_str() {
        "" => set("WT_SESSION"),
        term => !term.starts_with("screen") && !term.starts_with("tmux"),
    }
}

#[cfg(test)]
#[path = "pictures_tests.rs"]
mod pictures_tests;
