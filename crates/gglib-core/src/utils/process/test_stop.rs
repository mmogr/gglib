//! An orderly stop a test asks of a child, the same way on every OS.
//!
//! A test of how a program stops asks it the way a service manager or a
//! terminal does: SIGTERM on macOS and Linux, Ctrl-Break on Windows, each
//! of which [`super::stop::stop_requested`] hears as an orderly stop.
//! Windows sends Ctrl-Break only to a process group on the sender's own
//! console, so the child is started from [`stoppable`], and
//! [`ask_to_stop`] reaches it. A test that uses the two needs no gate.
//!
//! Compiled in this crate's own tests and with its `test-utils` feature,
//! which other crates enable from `[dev-dependencies]`.

use std::ffi::OsStr;
use std::io;
use std::process::{Child, Command};

/// A command for `program` whose child [`ask_to_stop`] reaches.
///
/// On macOS and Linux it is [`super::cmd`]'s. On Windows the child runs on
/// this process's console, in a process group of its own, where `cmd`
/// gives it a console of its own; so it opens a console window when this
/// process has no console.
pub fn stoppable(program: impl AsRef<OsStr>) -> Command {
    let command = super::cmd(program);
    #[cfg(windows)]
    let command = {
        let mut command = command;
        super::host::own_group_on_this_console(&mut command);
        command
    };
    command
}

/// Ask `child`, started from a [`stoppable`] command, for an orderly stop.
///
/// The stop is SIGTERM on macOS and Linux, and Ctrl-Break to the child's
/// process group on Windows. It returns once the stop is sent; the child
/// stops in its own time.
///
/// # Errors
///
/// When the OS will not send it, with the OS's error, and on an OS where
/// gglib starts no program (iOS), where it is
/// [`io::ErrorKind::Unsupported`].
pub fn ask_to_stop(child: &Child) -> io::Result<()> {
    send(child.id())
}

#[cfg(all(unix, not(target_os = "ios")))]
fn send(pid: u32) -> io::Result<()> {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;

    let pid = i32::try_from(pid).map_err(io::Error::other)?;
    kill(Pid::from_raw(pid), Signal::SIGTERM).map_err(io::Error::from)
}

#[cfg(windows)]
fn send(pid: u32) -> io::Result<()> {
    super::host::ctrl_break(pid)
}

#[cfg(not(any(windows, all(unix, not(target_os = "ios")))))]
fn send(_: u32) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "gglib starts no program on this OS",
    ))
}

#[cfg(test)]
#[path = "test_stop_tests.rs"]
mod tests;
