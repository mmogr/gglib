//! Starting the service, so it outlives the command that started it.
//!
//! The CLI and the desktop app both start `gglib daemon run` this way, and
//! nothing else in the workspace sets the process group or the creation
//! flags (clippy.toml).
//!
//! What keeps the service apart from the terminal that started it:
//!
//! - **macOS and Linux:** a process group of its own, so the terminal's
//!   Ctrl-C and hang-up go to the command in front, not to the service.
//! - **Windows:** `command` comes from [`super::cmd`], whose
//!   `CREATE_NO_WINDOW` gives the service a console of its own with no
//!   window. The terminal's Ctrl-C and its close button reach only the
//!   processes on the terminal's own console. The starting command's
//!   standard handles, a capturing caller's pipe among them, are kept from
//!   the service by [`super::host::prepare`], which `main` calls first.

use std::fs::File;
use std::process::{Child, Command, Stdio};

/// Start `command` as the service: with no input, writing to `log`, and
/// detached from the command that started it.
///
/// `command` comes from [`super::cmd`], with the service's arguments.
pub fn spawn_daemon(mut command: Command, log: &File) -> std::io::Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log.try_clone()?);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        #[expect(
            clippy::disallowed_methods,
            reason = "the service's process group is set here and nowhere else"
        )]
        command.process_group(0);
    }

    command.spawn()
}
