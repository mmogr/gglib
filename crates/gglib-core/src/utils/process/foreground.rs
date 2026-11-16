//! Letting another process bring a window to the front (#2046).
//!
//! Windows lets a process bring a window to the front only in a few cases
//! (`SetForegroundWindow`, Remarks): among them, it is the foreground
//! process, it was started by the foreground process, or it received the
//! last input. "A process that can set the foreground window can enable
//! another process to set the foreground window by calling
//! `AllowSetForegroundWindow`", and that process "loses the ability ... the
//! next time that either the user generates input, unless the input is
//! directed at that process, or the next time a process calls
//! `AllowSetForegroundWindow`, unless the same process is specified".
//!
//! The desktop app lends it to the service when the page posts a repair:
//! the service runs the installer of Microsoft's Visual C++ runtime, the
//! installer asks Windows for permission, and Windows leaves a prompt asked
//! for from the background flashing on the taskbar. The app's window has
//! just been clicked, so the app can lend the right. The prompt comes only
//! after the service has downloaded the installer (about 25 MB) and checked
//! it, and a click or key press anywhere in that time ends the right.
//!
//! [`allow_foreground`] is the same call on every OS; what it promises is
//! the [`Foreground`] it returns:
//!
//! - **Windows: [`Foreground::Allowed`]** when Windows granted it, and the
//!   error Windows gave otherwise: the caller could not set the foreground
//!   itself, which Windows documents, or no process has that id, which it
//!   does not.
//! - **macOS and Linux: [`Foreground::NotNeeded`].** They keep no such
//!   right, and nothing is granted.
//!
//! Two ids are refused on every OS before Windows is asked. `u32::MAX` is
//! Windows' `ASFW_ANY`, every process at once: never a process's id, and
//! lending every process the right is never what gglib means. `0` is the
//! System Idle Process, never a program gglib started, and Windows does not
//! say what a grant to it does.
//!
//! What Windows does not document is whether the right reaches the
//! processes the allowed one starts, or what decides where Windows opens its
//! permission prompt. That a prompt comes to the front is shown on a
//! Windows machine, not by this module's tests.

use std::io;

/// What a grant of the foreground did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Foreground {
    /// Windows granted it: the process may bring a window to the front
    /// until the user's next input that is not to it, or the next grant to
    /// another process (Windows).
    Allowed,
    /// There is no such right to grant, and nothing was (macOS, Linux).
    NotNeeded,
}

/// Windows' `ASFW_ANY`: every process at once.
const EVERY_PROCESS: u32 = u32::MAX;

/// The System Idle Process's id, which no program has.
const NO_PROGRAM: u32 = 0;

/// Let the process `pid` bring a window to the front, as this process may.
///
/// # Errors
///
/// `pid` is `u32::MAX` (every process) or `0` (no program), on every OS;
/// or, on Windows, Windows refused, with its reason: this process could not
/// set the foreground itself (documented), or no process has that id (not
/// documented). Nothing was granted.
pub fn allow_foreground(pid: u32) -> io::Result<Foreground> {
    if pid == EVERY_PROCESS || pid == NO_PROGRAM {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("a grant is made to one program, never to process id {pid}"),
        ));
    }
    #[cfg(windows)]
    {
        super::host::allow_set_foreground(pid)?;
        Ok(Foreground::Allowed)
    }
    #[cfg(not(windows))]
    {
        Ok(Foreground::NotNeeded)
    }
}

#[cfg(test)]
#[path = "foreground_tests.rs"]
mod tests;
