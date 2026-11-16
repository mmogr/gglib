//! The stand-in program: follows the instruction file beside it, which
//! `gglib_test_proc::instructions_path` names, and exits 0 at its end or
//! when there is none.
//!
//! A file that cannot be read, or holds a line that is no instruction, is
//! reported on stderr and exits 2; `install` refuses such a file before it
//! is written, so only a hand-written one gets here.

use std::io::Write as _;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use gglib_test_proc::{ARGS, Instruction, instructions_path, parse};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    match instructions() {
        Ok(list) => run(&list, &args),
        Err(why) => {
            eprintln!("gglib-test-proc: {why}");
            ExitCode::from(2)
        }
    }
}

/// The instructions beside this program; none when there is no file.
fn instructions() -> Result<Vec<Instruction>, String> {
    let program = std::env::current_exe().map_err(|e| format!("own path: {e}"))?;
    let file = instructions_path(&program);
    match std::fs::read_to_string(&file) {
        Ok(text) => {
            parse(&text).map_err(|line| format!("{}: not an instruction, {line}", file.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

fn run(list: &[Instruction], args: &[String]) -> ExitCode {
    let joined = args.join(" ");
    for instruction in list {
        let done = match instruction {
            Instruction::Argv(path) => write_argv(path, args),
            Instruction::Pid(path) => write_whole(path, &format!("{}\n", std::process::id())),
            Instruction::Print(text) => {
                writeln!(std::io::stdout(), "{}", text.replace(ARGS, &joined))
                    .and_then(|()| std::io::stdout().flush())
            }
            Instruction::Eprint(text) => {
                writeln!(std::io::stderr(), "{}", text.replace(ARGS, &joined))
            }
            Instruction::Sleep(seconds) => {
                std::thread::sleep(Duration::from_secs(*seconds));
                Ok(())
            }
            Instruction::Exit(code) => return ExitCode::from(*code),
        };
        if let Err(e) = done {
            eprintln!("gglib-test-proc: {instruction:?}: {e}");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

/// Writes each argument and a newline to `path`, whole.
fn write_argv(path: &Path, args: &[String]) -> std::io::Result<()> {
    let mut text = String::new();
    for arg in args {
        text.push_str(arg);
        text.push('\n');
    }
    write_whole(path, &text)
}

/// Writes `text` to `path`, whole: to a file beside it first, then renamed
/// onto it.
fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}
