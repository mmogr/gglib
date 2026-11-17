//! Running a program that is expected to answer and exit, with a time limit.
//!
//! `Command::output()` waits for as long as the program runs, and for as long
//! as anything holds its output open. A probe such as `llama-server
//! --version` that never exits, or leaves a process holding its stdout, kept
//! `gglib config llama validate`, the setup status and the commands that ask
//! it waiting with no end (#1984). [`run_bounded`] gives it `wait`, then
//! kills it and says so.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// How long a program asked what it is (`--version`) has to answer. Long
/// enough for a first run that an antivirus scans; far shorter than a hang.
pub const PROBE_WAIT: Duration = Duration::from_secs(10);

/// How often a running program is checked on.
const POLL: Duration = Duration::from_millis(10);

/// Run `command` with nothing on stdin and its output read, and give it
/// `wait` to exit.
///
/// Its stdout and stderr are what it printed. A program still running when
/// `wait` is up is killed, and the answer is an error of kind
/// [`std::io::ErrorKind::TimedOut`]. Output that something else keeps open
/// after the program has exited is read until `wait` is up, and no longer.
///
/// # Errors
///
/// The program could not be started, or it did not exit within `wait`.
pub fn run_bounded(command: &mut Command, wait: Duration) -> std::io::Result<Output> {
    let deadline = Instant::now() + wait;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = read_on_a_thread(child.stdout.take());
    let stderr = read_on_a_thread(child.stderr.take());

    let status = exit_by(&mut child, deadline, wait)?;

    // A moment past the deadline at least: a program that exits just in time
    // has its output read.
    let left = deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_millis(100));
    Ok(Output {
        status,
        stdout: stdout.recv_timeout(left).unwrap_or_default(),
        stderr: stderr.recv_timeout(left).unwrap_or_default(),
    })
}

/// Run `command` with nothing on stdin, stdout or stderr, and give it
/// `wait` to exit: for a program whose output is never read, so nothing it
/// leaves running can hold a pipe of gglib's open.
///
/// A program still running when `wait` is up is killed, and the answer is
/// an error of kind [`std::io::ErrorKind::TimedOut`]. Only the program
/// started is killed: one it started itself, such as an elevated copy of an
/// installer, may carry on.
///
/// # Errors
///
/// The program could not be started, or it did not exit within `wait`.
pub fn run_bounded_silent(
    command: &mut Command,
    wait: Duration,
) -> std::io::Result<std::process::ExitStatus> {
    let deadline = Instant::now() + wait;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    exit_by(&mut child, deadline, wait)
}

/// Wait for `child` to exit until `deadline`, `wait` after it was started;
/// kill it then, and say so as an error of kind
/// [`std::io::ErrorKind::TimedOut`].
fn exit_by(
    child: &mut std::process::Child,
    deadline: Instant,
    wait: Duration,
) -> std::io::Result<std::process::ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("did not answer within {} s", wait.as_secs_f32()),
            ));
        }
        thread::sleep(POLL);
    }
}

/// How a run of a program ended, as [`failure_kind`] reads it.
#[derive(Debug, Clone, Copy)]
pub enum Ended<'a> {
    /// It could not be started.
    NotStarted(&'a std::io::Error),
    /// It was still running when its wait was up, and was stopped.
    TimedOut,
    /// It exited with `code`, which is `None` when a signal ended it, having
    /// written `stderr`.
    Exited { code: Option<i32>, stderr: &'a str },
}

/// Why a run of a program failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Failure {
    /// There is no program at that path.
    #[error("it is not there")]
    NotFound,
    /// The path is not something this system runs: not executable, or not a
    /// file.
    #[error("it is not a program this system can run")]
    NotExecutable,
    /// It could not be started for another reason, which the system gave.
    #[error("it could not be started: {reason}")]
    CouldNotStart { reason: String },
    /// A library it needs was not found when the system loaded it. `name`
    /// is the library the loader named: Linux's and macOS's always do, so
    /// `None` is Windows, whose status says only that a DLL was not found.
    #[error("{}", missing_words(name.as_deref()))]
    MissingLibrary { name: Option<String> },
    /// A library it needs is there but could not be loaded: for another
    /// architecture, damaged, or refused. `name` as for
    /// [`Self::MissingLibrary`].
    #[error("{}", bad_words(name.as_deref()))]
    BadLibrary { name: Option<String> },
    /// A library the system provides is older than the program needs:
    /// `name` has no `version` of its symbols (`libm.so.6`, `GLIBC_2.43`).
    /// The program was built for a newer system, and installing it again
    /// brings the same build. Linux only: glibc's check of symbol versions.
    #[error(
        "this system's {name} is older than it needs: it asks for {version}, so it was built \
         for a newer system than this one"
    )]
    NewerLibraryNeeded { name: String, version: String },
    /// It did not exit within its wait.
    #[error("it did not answer in time")]
    TimedOut,
    /// It was ended by a crash: a Windows exception status in `code`, or a
    /// Unix signal, with no code.
    #[error("{}", crash_words(*code))]
    Crashed { code: Option<i32> },
    /// It ran and exited with `code`, not 0.
    #[error("it exited with status {code}")]
    Failed { code: i32 },
}

/// Windows' `STATUS_DLL_NOT_FOUND` ([MS-ERREF] 2.3.1): a DLL the program
/// needs was not found when it was loaded.
const STATUS_DLL_NOT_FOUND: u32 = 0xC000_0135;

/// Windows' `STATUS_INVALID_IMAGE_FORMAT` ([MS-ERREF] 2.3.1): an image it
/// loads is not one this system runs, such as an x86 DLL found by an x64
/// program.
const STATUS_INVALID_IMAGE_FORMAT: u32 = 0xC000_007B;

/// What glibc's loader writes before the library's name
/// (`fatal_error`, elf/dl-catch.c); it then exits 127 (elf/dl-printf.c).
const GLIBC_LOADER: &str = "error while loading shared libraries: ";

/// What glibc's loader says of a library it did not find (elf/dl-load.c).
/// Its other reasons (`wrong ELF class`, `invalid ELF header`, `file too
/// short`) are a library that is there.
const GLIBC_NOT_FOUND: &str = "cannot open shared object file";

/// What glibc's loader writes between a library's path and the version of
/// its symbols the program asks for, when the library has none of that
/// version. The line is the program, the library, then
/// ``version `GLIBC_2.43' not found (required by ./libfoo.so)``, each after
/// a colon and a space (`match_symbol`, elf/dl-version.c, printed by
/// `print_missing_version`, elf/rtld.c). `version_check_doit` (elf/rtld.c)
/// then exits 1. A `weak version` it only warns of has `weak ` before it.
const GLIBC_VERSION: &str = ": version `";

/// What follows the version in that line.
const GLIBC_VERSION_NOT_FOUND: &str = "' not found (required by ";

/// What macOS's dyld writes before the library's path, after `dyld: ` or
/// `dyld[<pid>]: `; it then aborts the program.
const DYLD_NOT_LOADED: &str = "Library not loaded: ";

/// What dyld says of a path it tried and found nothing at: macOS 12 and
/// later, and before.
const DYLD_NOT_FOUND: [&str; 2] = ["(no such file)", "image not found"];

/// What dyld says of a library built for another architecture, which can
/// sit in the list of tried paths beside ones with nothing there.
const DYLD_WRONG_ARCH: &str = "incompatible architecture";

/// Why a run of a program failed, from how it [`Ended`].
///
/// The one place that knows what each OS reports for a library that cannot
/// be loaded: Windows' exit status, glibc's loader's words with exit 127,
/// its words for a library too old with exit 1, and dyld's words. The
/// status is matched as a value, so every OS's report
/// is read on any OS. A run that exited 0 is not a failure and is not asked
/// about: [`failure_of`] answers `None` for it.
#[must_use]
pub fn failure_kind(ended: Ended<'_>) -> Failure {
    match ended {
        Ended::NotStarted(e) => match e.kind() {
            std::io::ErrorKind::NotFound => Failure::NotFound,
            std::io::ErrorKind::PermissionDenied => Failure::NotExecutable,
            std::io::ErrorKind::TimedOut => Failure::TimedOut,
            _ => Failure::CouldNotStart {
                reason: e.to_string(),
            },
        },
        Ended::TimedOut => Failure::TimedOut,
        Ended::Exited { code, stderr } => exited(code, stderr),
    }
}

/// Why `run`, the answer of [`run_bounded`], failed; `None` when the program
/// ran and exited 0.
#[must_use]
pub fn failure_of(run: &std::io::Result<Output>) -> Option<Failure> {
    match run {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(failure_kind(Ended::Exited {
            code: output.status.code(),
            stderr: &String::from_utf8_lossy(&output.stderr),
        })),
        Err(e) => Some(failure_kind(Ended::NotStarted(e))),
    }
}

fn exited(code: Option<i32>, stderr: &str) -> Failure {
    match code.map(i32::cast_unsigned) {
        Some(STATUS_DLL_NOT_FOUND) => return Failure::MissingLibrary { name: None },
        Some(STATUS_INVALID_IMAGE_FORMAT) => return Failure::BadLibrary { name: None },
        _ => {}
    }
    // Each loader is read as missing only on its own not-found words; any
    // other reason it gives (another architecture, a bad header, a refused
    // signature) is a library that is there and could not be loaded.
    if code == Some(127)
        && let Some(line) = line_after(stderr, GLIBC_LOADER)
    {
        let name = Some(line.split(':').next().unwrap_or(line).trim().to_owned());
        return if line.contains(GLIBC_NOT_FOUND) {
            Failure::MissingLibrary { name }
        } else {
            Failure::BadLibrary { name }
        };
    }
    if code == Some(1)
        && let Some(needed) = newer_library_needed(stderr)
    {
        return needed;
    }
    if let Some(line) = line_after(stderr, DYLD_NOT_LOADED) {
        let path = line.trim();
        let name = Some(path.rsplit('/').next().unwrap_or(path).to_owned());
        let not_found = DYLD_NOT_FOUND.iter().any(|words| stderr.contains(words));
        return if not_found && !stderr.contains(DYLD_WRONG_ARCH) {
            Failure::MissingLibrary { name }
        } else {
            Failure::BadLibrary { name }
        };
    }
    match code {
        // Unix exit codes are 0 to 255; a negative one is a Windows
        // exception status (0xC0000005, an access violation, and the like).
        Some(code) if code >= 0 => Failure::Failed { code },
        code => Failure::Crashed { code },
    }
}

/// The library and the version of the first line of `stderr` in which
/// glibc's loader says a library has no such version ([`GLIBC_VERSION`]).
fn newer_library_needed(stderr: &str) -> Option<Failure> {
    let (before, after) = stderr
        .lines()
        .find_map(|line| line.split_once(GLIBC_VERSION))?;
    let (version, _) = after.split_once(GLIBC_VERSION_NOT_FOUND)?;
    // `<program>: <library's path>`: the library is what follows the last
    // `: `, and its name what follows the last `/`.
    let library = before.rsplit(": ").next().unwrap_or(before);
    let name = library.rsplit('/').next().unwrap_or(library);
    Some(Failure::NewerLibraryNeeded {
        name: name.to_owned(),
        version: version.to_owned(),
    })
}

/// The rest of the first line of `text` that holds `marker`, after it.
fn line_after<'t>(text: &'t str, marker: &str) -> Option<&'t str> {
    text.lines()
        .find_map(|line| line.split_once(marker).map(|(_, rest)| rest))
}

/// Microsoft's permalink to its current Visual C++ Redistributable for x64.
///
/// A program built with Visual C++ needs it. It follows Microsoft's newest
/// release. The sentence for a missing library on Windows names it, and
/// gglib's repair of an engine downloads it from here.
pub const VISUAL_CPP_RUNTIME_X64_URL: &str = "https://aka.ms/vc14/vc_redist.x64.exe";

fn missing_words(name: Option<&str>) -> String {
    name.map_or_else(
        || {
            format!(
                "a library it needs is missing. Windows does not say which; for a program \
                 built with Visual C++ it is most likely the Microsoft Visual C++ \
                 Redistributable (x64), from Microsoft at \
                 {VISUAL_CPP_RUNTIME_X64_URL} (not the x86 one)"
            )
        },
        |name| format!("the library {name} it needs is missing"),
    )
}

fn bad_words(name: Option<&str>) -> String {
    let which = name.map_or_else(
        || "a library it needs (Windows does not say which)".to_owned(),
        |name| format!("the library {name} it needs"),
    );
    format!(
        "{which} is there but could not be loaded: it may be for another architecture, or damaged"
    )
}

fn crash_words(code: Option<i32>) -> String {
    code.map_or_else(
        || "it was ended by a signal".to_owned(),
        |code| format!("it crashed (status {:#010x})", code.cast_unsigned()),
    )
}

/// Read `stream` to its end on a thread of its own, so a stream something
/// keeps open cannot hold the caller.
fn read_on_a_thread(stream: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (sent, read) = mpsc::channel();
    if let Some(mut stream) = stream {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stream.read_to_end(&mut bytes);
            let _ = sent.send(bytes);
        });
    } else {
        let _ = sent.send(Vec::new());
    }
    read
}

#[cfg(test)]
#[path = "bounded_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bounded_failure_tests.rs"]
mod failure_tests;
