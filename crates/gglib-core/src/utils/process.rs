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

/// `CREATE_NO_WINDOW` — the process-creation flag that stops a child from
/// getting a console window. Named once here so the two spawn helpers below
/// cannot drift apart on the value.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW);
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
    #[allow(unused_mut)]
    let mut c = tokio::process::Command::new(program);
    #[cfg(windows)]
    {
        // No `CommandExt` import here: unlike `std`, tokio puts `creation_flags`
        // directly on its own `Command`, so importing the trait warns as unused.
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}
