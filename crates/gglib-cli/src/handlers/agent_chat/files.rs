//! A `gglib chat` or `gglib q` session's files: the folder gglib's own file
//! tools read, and the one stderr line the session starts with about it
//! ([ADR 0023], decision 2).
//!
//! A chat's folder is the directory the session was launched in, never home
//! or a system folder (`folder::launched`): its turns say it, as the chat
//! page's do, and the daemon remembers it on the chat. A resumed chat that
//! remembers a folder still there keeps it, the one the chat page chose
//! included, whatever it is: its turns say none, and the line names that
//! folder. One that remembers none, or one since gone, takes the launch
//! directory as a new chat does.
//!
//! [ADR 0023]: https://github.com/mmogr/gglib/blob/main/docs/adr/0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md

use std::io;
use std::path::{Path, PathBuf};

use gglib_core::domain::folder::{self, NotAdopted, Places};
use gglib_mcp::{BUILTIN_PREFIX, FILE_TOOLS};

/// Which chat a session is on, as its folder goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Chat<'a> {
    /// A chat this session makes.
    New,
    /// A chat it resumes, and the folder that chat remembers, if any.
    Resumed(Option<&'a str>),
}

/// The folder a session's turns say, and the line that says what it reads,
/// for a session with the `--tools` list `tools` and `--no-tools` as
/// `no_tools`; `sent_to` names the paired machine a `--remote` session's
/// turns are answered on.
///
/// The turns say where this process runs, by [`folder::launched`]'s rule:
/// never home or a system folder, where they say none. A resumed chat that
/// remembers a folder that is still one keeps it: its turns say none. A
/// session with `--no-tools`, or whose `--tools` list admits no file tool,
/// says none and nothing of files.
pub(crate) fn of_session(
    tools: &[String],
    no_tools: bool,
    sent_to: Option<&str>,
    chat: Chat<'_>,
) -> (Option<String>, Option<String>) {
    if no_tools || !admits_a_file_tool(tools) {
        return (None, None);
    }
    if let Chat::Resumed(Some(kept)) = chat
        && folder::chosen(Path::new(kept)).is_ok()
    {
        return (None, Some(files_line(&Ok(PathBuf::from(kept)), sent_to)));
    }
    let (root, line) = in_dir(std::env::current_dir(), &Places::current(), sent_to);
    (root.map(|dir| dir.display().to_string()), Some(line))
}

/// [`of_session`] for a session whose list admits a file tool, launched in
/// `cwd` as the process reads it on a machine whose places are `places`,
/// its turns sent to `sent_to`. A directory that cannot be read, deleted
/// since the shell entered it, is no folder: the session starts without
/// file tools and says why.
fn in_dir(
    cwd: io::Result<PathBuf>,
    places: &Places,
    sent_to: Option<&str>,
) -> (Option<PathBuf>, String) {
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
    let root = folder::launched_on(&cwd, places);
    let line = files_line(&root, sent_to);
    (root.ok(), line)
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
/// given the folder its file tools read, or why it has none (`root`), and,
/// under `--remote`, the paired machine its turns are sent to (`sent_to`).
///
/// A folder is named, read-only, with where what is read goes when that is
/// another machine. A session with no folder was launched in home or a
/// system folder, which are never read without being chosen, and says
/// which, and how to get one.
fn files_line(root: &Result<PathBuf, NotAdopted>, sent_to: Option<&str>) -> String {
    const HOW: &str = "cd into a project to read it, or --no-tools";
    match (root, sent_to) {
        (Ok(dir), None) => format!("Files: {} (read-only)", dir.display()),
        (Ok(dir), Some(machine)) => format!(
            "Files: {} on this machine (read-only; what is read is sent to {machine})",
            dir.display()
        ),
        (Err(NotAdopted::Home | NotAdopted::System(_)), _) => {
            format!("Files: off in your home folder or /; {HOW}")
        }
    }
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
