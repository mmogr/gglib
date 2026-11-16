//! Whether an engine's server binary works, for either engine.
//!
//! It is asked `--version`, and a run that fails is read by
//! [`failure_kind`]: the
//! one place that knows what each OS reports for a program that cannot
//! start. llama.cpp's status, stable-diffusion.cpp's status and an install's
//! Verify phase all check here, so each says the same thing for the same
//! binary. A `#[path]` child of `engine.rs`.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use gglib_core::domain::RuntimeKind;
use gglib_core::paths::{
    LLAMA_INSTALL_COMMAND, SD_INSTALL_COMMAND, llama_server_path, sd_server_path,
};
use gglib_core::utils::process::bounded::{
    Ended, Failure, PROBE_WAIT, failure_kind, failure_of, run_bounded,
};
use gglib_core::utils::process::cmd;

/// Why an engine's server binary does not work, worded with what to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotWorking {
    /// How asking it `--version` failed.
    pub failure: Failure,
    path: PathBuf,
    /// The command that installs it.
    install_command: &'static str,
    /// How many seconds it was given to answer.
    wait_secs: u64,
    /// The first line it wrote on stderr, for a run that exited or crashed.
    said: Option<String>,
}

impl std::error::Error for NotWorking {}

impl fmt::Display for NotWorking {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            failure,
            install_command,
            ..
        } = self;
        // The binary's name (`llama-server`), on every OS.
        let binary = self.path.file_stem().unwrap_or_default().to_string_lossy();
        let path = self.path.display();
        let reinstall = format!("Run '{install_command} --force' to reinstall it.");
        match failure {
            Failure::NotFound => write!(
                f,
                "{binary} not found at: {path}\n\nRun '{install_command}' to install it."
            ),
            Failure::NotExecutable => {
                write!(
                    f,
                    "{binary} is not a program this system can run: {path}\n\n{reinstall}"
                )
            }
            Failure::TimedOut => write!(
                f,
                "{binary} did not answer `--version` within {} s: {path}",
                self.wait_secs
            ),
            // A library the system has to provide is not brought back by a
            // reinstall, so one is suggested only for a library that may
            // have come with the download.
            Failure::MissingLibrary { name: None } | Failure::BadLibrary { name: None } => {
                write!(f, "{binary} cannot start: {failure}.\n\nBinary: {path}")
            }
            Failure::MissingLibrary { .. }
            | Failure::BadLibrary { .. }
            | Failure::CouldNotStart { .. } => write!(
                f,
                "{binary} cannot start: {failure}.\n\nBinary: {path}\n\
                 If it came with the download, run '{install_command} --force' to reinstall it."
            ),
            Failure::Crashed { .. } | Failure::Failed { .. } => {
                write!(f, "{binary} failed when asked `--version`: {failure}")?;
                if let Some(said) = &self.said {
                    write!(f, " ({said})")?;
                }
                write!(f, ": {path}\n\n{reinstall}")
            }
        }
    }
}

/// Check `kind`'s installed server binary: `Ok` when it answers
/// `--version`.
///
/// # Errors
///
/// Its path cannot be resolved, or it does not work ([`NotWorking`]).
pub fn validate_engine(kind: RuntimeKind) -> anyhow::Result<()> {
    let path = match kind {
        RuntimeKind::Llama => llama_server_path()?,
        RuntimeKind::StableDiffusion => sd_server_path()?,
    };
    validate_engine_at(kind, &path)?;
    Ok(())
}

/// Check `kind`'s server binary at `path`, and give what it answered.
///
/// # Errors
///
/// It does not work.
pub fn validate_engine_at(kind: RuntimeKind, path: &Path) -> Result<Output, NotWorking> {
    let install_command = match kind {
        RuntimeKind::Llama => LLAMA_INSTALL_COMMAND,
        RuntimeKind::StableDiffusion => SD_INSTALL_COMMAND,
    };
    validate_binary(path, install_command, PROBE_WAIT)
}

/// Ask the binary at `path` `--version`, giving it `wait`, and give what it
/// answered; `install_command` installs it.
pub(crate) fn validate_binary(
    path: &Path,
    install_command: &'static str,
    wait: Duration,
) -> Result<Output, NotWorking> {
    let not_working = |failure, said| NotWorking {
        failure,
        path: path.to_path_buf(),
        install_command,
        wait_secs: wait.as_secs(),
        said,
    };
    if !path.exists() {
        return Err(not_working(Failure::NotFound, None));
    }
    if !path.is_file() {
        return Err(not_working(Failure::NotExecutable, None));
    }
    let run = run_bounded(cmd(path).arg("--version"), wait);
    if let Some(failure) = failure_of(&run) {
        let said = run.ok().and_then(|output| first_line(&output.stderr));
        return Err(not_working(failure, said));
    }
    run.map_err(|e| not_working(failure_kind(Ended::NotStarted(&e)), None))
}

/// The first line of `bytes` with anything on it.
pub(crate) fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}
