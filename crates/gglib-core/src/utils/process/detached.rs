//! Starting the service, so it outlives the command that started it.

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

    // Detach from this command's process group so terminal signals (Ctrl-C)
    // sent to the foreground command never reach the daemon.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command.spawn()
}
