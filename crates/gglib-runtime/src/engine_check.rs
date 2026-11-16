//! Whether an engine's server binary works, for either engine.
//!
//! It is asked `--version`, and a run that fails is read by
//! [`failure_kind`]: the
//! one place that knows what each OS reports for a program that cannot
//! start. llama.cpp's status, stable-diffusion.cpp's status and an install's
//! Verify phase all check here, so each says the same thing for the same
//! binary. A binary the system calls busy is asked again for a moment
//! ([`BUSY_WAIT`]). A `#[path]` child of `engine.rs`.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use crate::process::said_line;

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

impl NotWorking {
    /// `kind`'s binary at `path` failing as `failure` when it was started
    /// to serve, worded as the check words it: a load that could not start
    /// it says what its status would.
    pub(crate) fn when_started(kind: RuntimeKind, path: &Path, failure: Failure) -> Self {
        Self {
            failure,
            path: path.to_path_buf(),
            install_command: install_command_of(kind),
            wait_secs: PROBE_WAIT.as_secs(),
            said: None,
        }
    }
}

/// The command that installs `kind`.
const fn install_command_of(kind: RuntimeKind) -> &'static str {
    match kind {
        RuntimeKind::Llama => LLAMA_INSTALL_COMMAND,
        RuntimeKind::StableDiffusion => SD_INSTALL_COMMAND,
    }
}

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
    validate_binary(path, install_command_of(kind), PROBE_WAIT)
}

/// The binaries this check last found unable to start ([`cannot_start`]),
/// in this process.
///
/// A load reads it ([`refuse_if_known_not_working`]) so it need not run the
/// check on every launch: a binary not in it is started as it is. One whose
/// `--version` only timed out, crashed or failed is not in it, and its load
/// is not refused.
static FOUND_NOT_WORKING: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Mutex::default);

fn found_not_working() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    FOUND_NOT_WORKING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Refuse a load of `kind`'s binary at `path` when this check last found it
/// not working, in the words it gives.
///
/// Such a binary is asked again first, so the refusal is never stale: a
/// library put back or a runtime installed since leaves the binary as it
/// was, and the check is all that can tell. Running a binary that cannot
/// start costs milliseconds. A binary the check has not found unable to
/// start, or found working since, is not run: a load of it is not delayed,
/// and a server that then fails says why itself.
///
/// Blocking, when the binary is asked again.
///
/// # Errors
///
/// It still cannot start. Asked again, a binary that now only times out,
/// crashes or fails is let through to its load.
pub(crate) fn refuse_if_known_not_working(
    kind: RuntimeKind,
    path: &Path,
) -> Result<(), NotWorking> {
    if !found_not_working().contains(path) {
        return Ok(());
    }
    match validate_engine_at(kind, path) {
        Err(not_working) if cannot_start(&not_working.failure) => Err(not_working),
        _ => Ok(()),
    }
}

/// Ask the binary at `path` `--version`, giving it `wait`, and give what it
/// answered; `install_command` installs it. What it found is remembered for
/// [`refuse_if_known_not_working`].
pub(crate) fn validate_binary(
    path: &Path,
    install_command: &'static str,
    wait: Duration,
) -> Result<Output, NotWorking> {
    let answer = check_binary(path, install_command, wait);
    match &answer {
        Err(not_working) if cannot_start(&not_working.failure) => {
            found_not_working().insert(path.to_path_buf());
        }
        _ => {
            found_not_working().remove(path);
        }
    }
    answer
}

fn check_binary(
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
    let run = once_not_busy(BUSY_WAIT, || run_bounded(cmd(path).arg("--version"), wait));
    if let Some(failure) = failure_of(&run) {
        let said = run.ok().and_then(|output| first_line(&output.stderr));
        return Err(not_working(failure, said));
    }
    run.map_err(|e| not_working(failure_kind(Ended::NotStarted(&e)), None))
}

/// How long a binary the system will not start because it is busy is asked
/// again for.
///
/// Linux will not start a program from a file any process holds open for
/// writing ("text file busy"). An install writes the server and runs it at
/// once, at its Verify phase. A program gglib starts on another thread at
/// the moment the server is written (an engine, an MCP server, another
/// check) begins as a copy of this process, holding every descriptor open
/// then, the one the server is written through among them, until it starts
/// its own program a moment later; so Verify can meet a busy server that is
/// fine (#2045). Once a run has started, no such copy is left. The check is
/// the one place that sees the system's reason as a kind rather than words,
/// and every run of it, Verify's included, comes through here.
const BUSY_WAIT: Duration = Duration::from_secs(2);

/// How often a busy binary is asked again.
const BUSY_POLL: Duration = Duration::from_millis(10);

/// What `run` answers once it no longer fails because its program is busy,
/// or that failure once `limit` is up. Any other answer is given at once.
pub(super) fn once_not_busy<T>(
    limit: Duration,
    mut run: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let until = Instant::now() + limit;
    loop {
        match run() {
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy && Instant::now() < until =>
            {
                std::thread::sleep(BUSY_POLL);
            }
            answer => return answer,
        }
    }
}

/// The first line of `bytes` with anything on it, made plain and cut as a
/// line of a server's output is ([`said_line`]): what an engine prints is
/// shown, and carries no terminal control.
pub(crate) fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(said_line)
        .find(|line| !line.is_empty())
}

/// Whether `failure` proves the binary cannot start, whatever it is asked:
/// it is not there, not a program, missing or refused a library, or the
/// system would not start it. A run that timed out, crashed or exited
/// non-zero on `--version` does not: a scanner or a loaded machine can slow
/// the first, and a server may still load where its `--version` failed.
fn cannot_start(failure: &Failure) -> bool {
    matches!(
        failure,
        Failure::NotFound
            | Failure::NotExecutable
            | Failure::MissingLibrary { .. }
            | Failure::BadLibrary { .. }
            | Failure::CouldNotStart { .. }
    )
}
