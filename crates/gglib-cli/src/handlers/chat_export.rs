//! `gglib chat export`: a conversation written to the JSON file the chat
//! page's Export saves, its images in it.
//!
//! Both write what [`AppCore::export_chat`] answers, in the same text
//! ([`ChatExport::text`]), so a file from either reads the same.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use gglib_core::domain::chat_export::{ChatExport, unread_sentence};
use gglib_core::ports::chat_history::ChatHistoryError;
use gglib_core::services::AppCore;

use super::attachment::write;
use crate::bootstrap::CliContext;

/// Run `gglib chat export <id> [path]`, and say where it went, and which
/// images are in it by their id alone.
///
/// # Errors
///
/// As [`save`].
pub(crate) async fn execute(
    ctx: &CliContext,
    id: i64,
    path: Option<&Path>,
    force: bool,
) -> Result<()> {
    let (saved, export) = save(&ctx.app, id, path, force).await?;
    println!("Saved {}", saved.display());
    if let Some(sentence) = unread_sentence(export.unread()) {
        eprintln!("{sentence}");
    }
    Ok(())
}

/// Write conversation `id` to `path`, or to `conversation-<id>.json` in the
/// working directory, or in `path` when it is a directory. An existing file
/// is replaced only when `force`. Answers where it went, and what it wrote.
///
/// # Errors
///
/// When no conversation has the id, the chat history cannot be read, the
/// file exists and `force` is not set, or the file cannot be written.
pub(crate) async fn save(
    app: &AppCore,
    id: i64,
    path: Option<&Path>,
    force: bool,
) -> Result<(PathBuf, ChatExport)> {
    let export = app.export_chat(id).await.map_err(|err| match err {
        ChatHistoryError::ConversationNotFound(_) => {
            anyhow!("No conversation has the ID {id}; `gglib chat history` lists them.")
        }
        other => other.into(),
    })?;
    let name = export.file_name();
    let target = match path {
        None => PathBuf::from(name),
        Some(dir) if dir.is_dir() => dir.join(name),
        Some(file) => file.to_owned(),
    };
    write(&target, export.text().as_bytes(), force)?;
    Ok((target, export))
}

#[cfg(test)]
#[path = "chat_export_tests.rs"]
mod chat_export_tests;
