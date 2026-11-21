#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]

//! A test that needs a child process installs this crate's program under the
//! name the code under test looks for, with the instructions it is to follow,
//! and gets a program that runs the same way on every OS.
//!
//! [`install`] puts the program at `dir/<name>` (`<name>.exe` on Windows)
//! and writes the instructions to `dir/<name>.proc`. When the program runs it
//! reads the file named by [`instructions_path`] of its own path and follows
//! [`Instruction`]s one per line, in order. With no file beside it, it exits 0.

use std::env::consts::EXE_SUFFIX;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// One line of an instruction file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    /// `argv <path>`: write the program's arguments, one per line, to
    /// `path`. The file appears whole: it is written beside `path` and then
    /// renamed onto it, so a test polling for it never reads half of it.
    Argv(PathBuf),
    /// `calls <path>`: add one line to the end of `path`, the program's
    /// arguments joined by single spaces. Each start adds its own, so the
    /// file holds every call in the order they were made, where `argv`
    /// holds the last.
    Calls(PathBuf),
    /// `pid <path>`: write the program's own process id to `path`, whole,
    /// as `argv` writes, so a test can find the program wherever it was
    /// started from (behind a wrapper, say).
    Pid(PathBuf),
    /// `touch <path>`: write an empty file at `path`, as a sign the program
    /// got this far (say, to a clean exit).
    Touch(PathBuf),
    /// `read`: read stdin until it ends, as a server waits for its client
    /// to close its input.
    Read,
    /// `print <text>`: write `text` and a newline to stdout.
    Print(String),
    /// `eprint <text>`: write `text` and a newline to stderr.
    Eprint(String),
    /// `sleep <seconds>`: sleep that many whole seconds.
    Sleep(u64),
    /// `exit <code>`: exit at once with `code`.
    Exit(u8),
}

/// In the text of `print` and `eprint`, this stands for the program's
/// arguments joined by single spaces.
pub const ARGS: &str = "{args}";

/// Parses an instruction file: one [`Instruction`] per line, blank lines
/// skipped. The error names the first line that is not one.
pub fn parse(text: &str) -> Result<Vec<Instruction>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(n, line)| parse_line(line).ok_or_else(|| format!("line {}: {line:?}", n + 1)))
        .collect()
}

fn parse_line(line: &str) -> Option<Instruction> {
    let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
    Some(match verb {
        "argv" if !rest.is_empty() => Instruction::Argv(PathBuf::from(rest)),
        "calls" if !rest.is_empty() => Instruction::Calls(PathBuf::from(rest)),
        "pid" if !rest.is_empty() => Instruction::Pid(PathBuf::from(rest)),
        "touch" if !rest.is_empty() => Instruction::Touch(PathBuf::from(rest)),
        "read" if rest.is_empty() => Instruction::Read,
        "print" => Instruction::Print(rest.to_owned()),
        "eprint" => Instruction::Eprint(rest.to_owned()),
        "sleep" => Instruction::Sleep(rest.parse().ok()?),
        "exit" => Instruction::Exit(rest.parse().ok()?),
        _ => return None,
    })
}

/// The instruction file of the program at `program`.
///
/// It is the program's path with the OS's executable suffix (`.exe` on
/// Windows, nothing elsewhere) taken off, and `.proc` put on:
/// `dir/llama-server.exe` and `dir/llama-server` both read
/// `dir/llama-server.proc`.
pub fn instructions_path(program: &Path) -> PathBuf {
    let name = program.file_name().unwrap_or_default().to_string_lossy();
    let stem = name.strip_suffix(EXE_SUFFIX).unwrap_or(&name);
    let mut file = OsString::from(stem);
    file.push(".proc");
    program.with_file_name(file)
}

/// Installs the program as `dir/<name>` plus the OS's executable suffix,
/// with `instructions` as its instruction file, and returns its path.
///
/// The program is a copy of the built one, so it finds its instructions by
/// its own path, and has the name the code under test looks for wherever
/// that code resolves links. A symbolic link would not do: Linux reports a
/// program's path with links resolved. Nor would a hard link: under load,
/// macOS can report a hard-linked program's path as another of its names, so
/// it reads another program's instructions (1 run in 400, measured with 12
/// at a time). Before its instructions are written the copy is run once (see
/// [`run_once`]). An earlier install of the same name in `dir` is replaced.
///
/// # Panics
///
/// When `name` is not a plain file name, `instructions` is not a list of
/// [`Instruction`]s, the program cannot be built or run, or a file cannot be
/// written. All are mistakes in the test, or a machine that cannot run it.
pub fn install(dir: &Path, name: &str, instructions: &str) -> PathBuf {
    assert!(
        !name.is_empty() && Path::new(name).file_name() == Some(name.as_ref()),
        "gglib-test-proc: {name:?} is not a file name"
    );
    if let Err(line) = parse(instructions) {
        panic!("gglib-test-proc: not an instruction, {line}");
    }
    let program = dir.join(format!("{name}{EXE_SUFFIX}"));
    let file = instructions_path(&program);
    remove(&program);
    remove(&file);
    std::fs::copy(built(), &program)
        .unwrap_or_else(|e| panic!("gglib-test-proc: copying to {}: {e}", program.display()));
    run_once(&program);
    std::fs::write(&file, instructions)
        .unwrap_or_else(|e| panic!("gglib-test-proc: writing {}: {e}", file.display()));
    program
}

/// Writes a wrapper script in `dir` that runs `program` as a shim does, and
/// returns its path.
///
/// The script starts `program` with its own arguments, waits for it and
/// exits with its status, as `npx.cmd` does: `<name>.cmd` on Windows, which
/// Rust starts through `cmd.exe`, and an executable `sh` script `<name>`
/// elsewhere. Neither hands its process over to the program, so the program
/// runs as the wrapper's child.
///
/// The `sh` script is written by a child `sh`, never through a descriptor of
/// this process, so it can be started the moment this returns: Linux would
/// refuse it as "text file busy" while a process another thread was just
/// starting still held such a descriptor.
///
/// # Panics
///
/// When the script cannot be written or made executable.
pub fn wrap(dir: &Path, name: &str, program: &Path) -> PathBuf {
    let program = program.display();
    let (wrapper, script) = if cfg!(windows) {
        (
            dir.join(format!("{name}.cmd")),
            format!("@\"{program}\" %*\r\n@exit /b %errorlevel%\r\n"),
        )
    } else {
        (
            dir.join(name),
            format!("#!/bin/sh\n\"{program}\" \"$@\"\nstatus=$?\nexit $status\n"),
        )
    };
    write_script(&wrapper, &script);
    wrapper
}

/// Writes `script` to `path` as an executable file, by a child `sh`.
///
/// Linux will not start a program from a file any process holds open for
/// writing ("text file busy"). A file this process wrote itself can be held
/// so after it is closed: a process another thread starts at that moment
/// begins as a copy of this one, holding every descriptor open then, until
/// it starts its own program. [`install`] waits that out by running its
/// copy once; a wrapper cannot be run once, since that runs the program. So
/// the wrapper is written by a child, and only that child and the `cat` it
/// starts ever hold it open for writing. Both have exited when this
/// returns, and no descriptor to it was ever in this process to be copied.
#[cfg(unix)]
fn write_script(path: &Path, script: &str) {
    use std::io::Write;

    let mut writer = command("/bin/sh")
        .args(["-c", "cat > \"$1\" && chmod 755 \"$1\"", "sh"])
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| {
            panic!(
                "gglib-test-proc: starting sh to write {}: {e}",
                path.display()
            )
        });
    writer
        .stdin
        .take()
        .expect("sh's stdin")
        .write_all(script.as_bytes())
        .unwrap_or_else(|e| panic!("gglib-test-proc: writing {}: {e}", path.display()));
    let status = writer
        .wait()
        .unwrap_or_else(|e| panic!("gglib-test-proc: writing {}: {e}", path.display()));
    assert!(
        status.success(),
        "gglib-test-proc: writing {}: sh {status}",
        path.display()
    );
}

/// Writes `script` to `path`. Windows runs a `.cmd` through `cmd.exe`,
/// which reads it; nothing refuses to start a file that was open for
/// writing.
#[cfg(not(unix))]
fn write_script(path: &Path, script: &str) {
    std::fs::write(path, script)
        .unwrap_or_else(|e| panic!("gglib-test-proc: writing {}: {e}", path.display()));
}

/// Removes `path`, if there is one.
fn remove(path: &Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("gglib-test-proc: removing {}: {e}", path.display()),
    }
}

/// The built program, built once per test binary.
///
/// A crate's tests cannot name another package's binary (cargo's
/// `CARGO_BIN_EXE_<name>` covers only the package's own), and where cargo
/// puts it depends on the target directory, the profile and the target. So
/// this asks the cargo that is running the tests (`CARGO`, which cargo sets
/// for them; `cargo` on the `PATH` without it) to build it, from this
/// crate's manifest, and reads the path from cargo's own report. The build
/// inherits the caller's environment, so it lands in the same target
/// directory with the same flags, and when the program is already built
/// cargo only says where it is.
fn built() -> &'static Path {
    static PROGRAM: OnceLock<PathBuf> = OnceLock::new();
    PROGRAM.get_or_init(build)
}

/// A command that starts `program`: the one place this crate starts one.
#[expect(
    clippy::disallowed_methods,
    reason = "gglib-test-proc has no dependencies, so it cannot take gglib_core::utils::process; a test program needs no Windows console flag"
)]
fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    Command::new(program)
}

fn build() -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let output = command(&cargo)
        .args([
            "build",
            "--locked",
            "--message-format=json",
            "--bin",
            "gglib-test-proc",
        ])
        .arg("--manifest-path")
        .arg(&manifest)
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("gglib-test-proc: running {}: {e}", cargo.display()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let built = stdout.lines().filter_map(executable).next_back();
    match built {
        Some(path) if output.status.success() => path,
        _ => panic!(
            "gglib-test-proc: `cargo build --bin gglib-test-proc` {} and named no program:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

/// Runs a new copy once, with no instruction file beside it, so it exits 0.
///
/// macOS checks a new executable file the first time it runs, about a
/// quarter of a second, reading it by the path it was started from. Run
/// first by the code under test, the check can find the test's directory
/// already deleted and fail, and be made again at each later run, so the
/// stand-ins stall one after another; run here, it is done while the
/// directory is certainly there, and before a test's own time limits start.
///
/// On Linux a copy can be refused for a moment as "text file busy", while a
/// process forked elsewhere in the test binary still holds the descriptor it
/// was written through; that is retried. Once a run has started, no such
/// descriptor is left to reach the test's own, and the copy is never opened
/// for writing again.
fn run_once(program: &Path) {
    let mut busy = 0;
    let ran = loop {
        match command(program).stdin(Stdio::null()).output() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy && busy < 100 => {
                busy += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            done => {
                break done.unwrap_or_else(|e| {
                    panic!("gglib-test-proc: running {}: {e}", program.display())
                });
            }
        }
    };
    assert!(
        ran.status.success(),
        "gglib-test-proc: {} {}:\n{}",
        program.display(),
        ran.status,
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// The `executable` of one line of cargo's JSON messages, when it names one.
fn executable(line: &str) -> Option<PathBuf> {
    const KEY: &str = "\"executable\":\"";
    let start = line.find(KEY)? + KEY.len();
    json_string(&line[start..]).map(PathBuf::from)
}

/// The JSON string whose opening quote comes just before `text`, unescaped;
/// `None` when it is not closed or holds an escape JSON does not have.
fn json_string(text: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => out.push(match chars.next()? {
                '"' => '"',
                '\\' => '\\',
                '/' => '/',
                'b' => '\u{8}',
                'f' => '\u{c}',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?
                }
                _ => return None,
            }),
            c => out.push(c),
        }
    }
    None
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
