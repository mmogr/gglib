//! A `gglib chat` session's files: the folder gglib's own file tools read,
//! which is the directory the session was launched in, and the one stderr
//! line the session starts with about it.

use std::io;
use std::path::{Path, PathBuf};

use gglib_core::domain::folder;
use gglib_mcp::{BUILTIN_PREFIX, FILE_TOOLS};

use super::config::AgentSessionParams;
use crate::target::Target;

/// The folder a session's file tools read, and the line that says so.
///
/// The folder is where this process runs, every invocation, `--continue`
/// included, by [`folder::launched`]'s rule: never home or `/`. A session
/// whose `--tools` list admits no file tool has none, and its directory is
/// not read.
pub(super) fn of_session(params: &AgentSessionParams) -> (Option<PathBuf>, Option<String>) {
    if !admits_a_file_tool(&params.tools) {
        return (None, None);
    }
    in_dir(std::env::current_dir(), sent_to(params))
}

/// [`of_session`] for a session whose list admits a file tool, launched in
/// `cwd` as the process reads it, its turns sent to `sent_to`. A directory
/// that cannot be read, deleted since the shell entered it, is no folder:
/// the session starts without file tools and says why.
fn in_dir(cwd: io::Result<PathBuf>, sent_to: Option<&str>) -> (Option<PathBuf>, Option<String>) {
    let cwd = match cwd {
        Ok(cwd) => cwd,
        Err(e) => {
            let line = format!(
                "Files: off; the folder this ran in cannot be read ({e}). cd into a project to \
                 read it, or --no-tools"
            );
            return (None, Some(line));
        }
    };
    let root = folder::launched(&cwd);
    let line = files_line(root.as_deref(), sent_to);
    (root, Some(line))
}

/// The paired machine a `--remote` session's turns, and what its file tools
/// read, are sent to; `None` for a session on this machine.
fn sent_to(params: &AgentSessionParams) -> Option<&str> {
    (params.target == Target::Remote).then(|| {
        let machine = params
            .turn
            .as_ref()
            .and_then(|turn| turn.machine.as_deref());
        machine.unwrap_or("the paired machine")
    })
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
