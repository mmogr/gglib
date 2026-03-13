//! Process spawning utilities with consistent cross-platform behaviour.
//!
//! On Windows, every child process created with `std::process::Command::new`
//! inherits a new console window unless `CREATE_NO_WINDOW` is explicitly set.
//! The `windows_subsystem = "windows"` attribute on the main binary only
//! suppresses the window for the *main* process — not any child processes.
//!
//! Use [`cmd`] and [`async_cmd`] instead of `Command::new` at every call site.
//! The Windows-specific flag is applied here and nowhere else.

use std::ffi::OsStr;

/// Create a [`std::process::Command`] that will not open a console window on Windows.
///
/// Identical to `std::process::Command::new(program)` on macOS and Linux.
///
/// # Usage
///
/// ```rust,ignore
/// use gglib_core::utils::process::cmd;
///
/// let output = cmd("nvidia-smi").arg("--list-gpus").output()?;
/// ```
pub fn cmd(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    c
}

/// Create a [`tokio::process::Command`] that will not open a console window on Windows.
///
/// Identical to `tokio::process::Command::new(program)` on macOS and Linux.
///
/// # Usage
///
/// ```rust,ignore
/// use gglib_core::utils::process::async_cmd;
///
/// let child = async_cmd("llama-server").arg("--port").arg("8080").spawn()?;
/// ```
pub fn async_cmd(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut c = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    c
}
