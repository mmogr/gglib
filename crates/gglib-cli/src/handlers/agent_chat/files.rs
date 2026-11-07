//! A `gglib chat` session's files: the folder gglib's own file tools read,
//! and the one stderr line the session starts with about it ([ADR 0023],
//! decision 2).
//!
//! A new chat's folder is the directory the session was launched in, never
//! home or `/`: its turns say it, as the chat page's do, and the daemon
//! remembers it on the chat. A resumed chat keeps the folder it remembers,
//! the one the chat page chose included: its turns say none, and the line
//! names that folder.
//!
//! [ADR 0023]: https://github.com/mmogr/gglib/blob/main/docs/adr/0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md

use std::io;
use std::path::{Path, PathBuf};

use gglib_core::domain::folder;
use gglib_mcp::{BUILTIN_PREFIX, FILE_TOOLS};

use crate::handlers::inference::chat::ChatArgs;

/// Which chat a session is on, as its folder goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Chat<'a> {
    /// A chat this session makes.
    New,
    /// A chat it resumes, and the folder that chat remembers, if any.
    Resumed(Option<&'a str>),
}

/// The folder a session's turns say, and the line that says what it reads;
/// `sent_to` names the paired machine a `--remote` session's turns are
/// answered on.
///
/// A new chat's turns say where this process runs, by [`folder::launched`]'s
/// rule: never home or `/`, where they say none. A resumed chat's say none,
/// and it reads the folder it keeps. A session with `--no-tools`, or whose
/// `--tools` list admits no file tool, says none and nothing of files.
pub(super) fn of_session(
    args: &ChatArgs,
    sent_to: Option<&str>,
    chat: Chat<'_>,
) -> (Option<String>, Option<String>) {
    if args.no_tools || !admits_a_file_tool(&args.tools) {
        return (None, None);
    }
    if let Chat::Resumed(kept) = chat {
        return (None, Some(kept_line(kept, sent_to)));
    }
    let (root, line) = in_dir(std::env::current_dir(), sent_to);
    (root.map(|dir| dir.display().to_string()), Some(line))
}

/// What a resumed chat says of the folder it keeps.
fn kept_line(kept: Option<&str>, sent_to: Option<&str>) -> String {
    kept.map_or_else(
        || "Files: off; this chat keeps no folder (the chat page chooses one)".to_owned(),
        |dir| files_line(Some(Path::new(dir)), sent_to),
    )
}

/// [`of_session`] for a session whose list admits a file tool, launched in
/// `cwd` as the process reads it, its turns sent to `sent_to`. A directory
/// that cannot be read, deleted since the shell entered it, is no folder:
/// the session starts without file tools and says why.
fn in_dir(cwd: io::Result<PathBuf>, sent_to: Option<&str>) -> (Option<PathBuf>, String) {
    let cwd = match cwd {
        Ok(cwd) => cwd,
        Err(e) => {
            let line = format!(
                "Files: off; the folder this ran in cannot be read ({e}). cd into a project to \
                 read it, or --no-tools"
            );
            return (None, line);
        }
    };
    let root = folder::launched(&cwd);
    let line = files_line(root.as_deref(), sent_to);
    (root, line)
}

/// Whether a session's tool allowlist lets a file tool ([`FILE_TOOLS`])
/// through: every tool (an empty list), or a list naming one, bare or as
/// `builtin:<name>`. A session whose list admits none is given no folder.
pub(super) fn admits_a_file_tool(tools: &[String]) -> bool {
    tools.is_empty()
        || tools.iter().any(|name| {
            let bare = name.strip_prefix(BUILTIN_PREFIX).unwrap_or(name);
            FILE_TOOLS.contains(&bare)
        })
}

/// What a session whose list admits a file tool says about its files,
/// given the folder its file tools read (`root`) and, under `--remote`, the
/// paired machine its turns are sent to (`sent_to`).
///
/// A folder is named, read-only, with where what is read goes when that is
/// another machine. A session with no folder was launched in home or `/`,
/// which are never read implicitly, and says how to get one.
fn files_line(root: Option<&Path>, sent_to: Option<&str>) -> String {
    match (root, sent_to) {
        (Some(dir), None) => format!("Files: {} (read-only)", dir.display()),
        (Some(dir), Some(machine)) => format!(
            "Files: {} on this machine (read-only; what is read is sent to {machine})",
            dir.display()
        ),
        (None, _) => {
            "Files: off in your home folder or /; cd into a project to read it, or --no-tools"
                .to_owned()
        }
    }
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
