#![doc = include_str!("README.md")]

pub mod bounded;
pub mod detached;
pub mod held;
pub mod host;
pub mod stop;
pub mod tied;

use std::ffi::OsStr;

/// `CREATE_NO_WINDOW` — the process-creation flag that stops a child from
/// getting a console window. Named once here so the two spawn helpers below
/// cannot drift apart on the value.
///
/// It also gives a console program a console of its own, with no window, in
/// place of its parent's. So the service, started by
/// [`detached::spawn_daemon`], which sets it too, is not stopped by a Ctrl-C
/// or a closed window in the terminal that started it.
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
#[expect(
    clippy::disallowed_methods,
    reason = "the one place a std Command is made; clippy.toml sends every other caller here"
)]
pub fn cmd(program: impl AsRef<OsStr>) -> std::process::Command {
    let command = std::process::Command::new(program);
    #[cfg(windows)]
    let command = {
        use std::os::windows::process::CommandExt;
        let mut command = command;
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    command
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
#[expect(
    clippy::disallowed_methods,
    reason = "the one place a tokio Command is made; clippy.toml sends every other caller here"
)]
pub fn async_cmd(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let command = tokio::process::Command::new(program);
    #[cfg(windows)]
    let command = {
        // No `CommandExt` import here: unlike `std`, tokio puts `creation_flags`
        // directly on its own `Command`, so importing the trait warns as unused.
        let mut command = command;
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    command
}
