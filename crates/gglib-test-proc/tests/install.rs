//! The installed program, run as a test runs it: built by `install` on first
//! use, under the name it was given, following its instruction file.

#![expect(
    clippy::disallowed_methods,
    reason = "gglib-test-proc has no dependencies, so it cannot take gglib_core::utils::process; a test program needs no Windows console flag"
)]

use std::process::Command;
use std::time::{Duration, Instant};

use gglib_test_proc::{install, instructions_path};

#[test]
fn the_program_follows_its_instructions_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let argv = dir.path().join("argv.txt");
    let program = install(
        dir.path(),
        "llama-server",
        &format!(
            "argv {}\nprint out {{args}}\neprint err {{args}}\nexit 7\nprint never\n",
            argv.display()
        ),
    );

    let ran = Command::new(&program)
        .args(["-m", "a model.gguf", "--port", "9000"])
        .output()
        .unwrap();

    assert_eq!(
        program.file_name().unwrap().to_string_lossy(),
        format!("llama-server{}", std::env::consts::EXE_SUFFIX)
    );
    assert_eq!(ran.status.code(), Some(7));
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "out -m a model.gguf --port 9000\n"
    );
    assert!(
        String::from_utf8_lossy(&ran.stderr).starts_with("err -m a model.gguf --port 9000"),
        "{ran:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&argv).unwrap(),
        "-m\na model.gguf\n--port\n9000\n"
    );
}

#[test]
fn with_no_instruction_file_the_program_exits_0() {
    let dir = tempfile::tempdir().unwrap();
    let program = install(dir.path(), "sd-server", "exit 9");
    std::fs::remove_file(instructions_path(&program)).unwrap();

    let ran = Command::new(&program).output().unwrap();

    assert_eq!(ran.status.code(), Some(0));
    assert!(ran.stdout.is_empty() && ran.stderr.is_empty(), "{ran:?}");
}

#[test]
fn the_program_sleeps_until_it_is_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let program = install(dir.path(), "server", "sleep 30");
    let mut child = Command::new(&program).spawn().unwrap();

    std::thread::sleep(Duration::from_millis(300));
    let running = child.try_wait().unwrap().is_none();
    child.kill().unwrap();
    let stopped = Instant::now();
    child.wait().unwrap();

    assert!(running, "still sleeping after 300 ms");
    assert!(stopped.elapsed() < Duration::from_secs(10));
}

/// Two programs in one directory each read their own file, and installing a
/// name again replaces the program and its instructions.
#[test]
fn each_name_reads_its_own_instructions() {
    let dir = tempfile::tempdir().unwrap();
    let first = install(dir.path(), "first", "exit 1");
    let second = install(dir.path(), "second", "exit 2");
    let again = install(dir.path(), "first", "exit 3");

    let code = |program| Command::new(program).status().unwrap().code();

    assert_eq!(again, first);
    assert_eq!(code(&first), Some(3));
    assert_eq!(code(&second), Some(2));
}

#[test]
#[should_panic(expected = "not an instruction, line 1")]
fn a_line_that_is_no_instruction_is_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "server", "exec sleep 60");
}

/// A wrapper runs the program as its child, passes its arguments on and
/// exits with its status; the program's `pid` file names the program, not
/// the wrapper.
#[test]
fn a_wrapper_runs_the_program_as_its_child_and_the_program_says_its_pid() {
    let dir = tempfile::tempdir().unwrap();
    let pid = dir.path().join("server.pid");
    let program = install(
        dir.path(),
        "server",
        &format!("pid {}\nprint out {{args}}\nexit 4", pid.display()),
    );
    let wrapper = gglib_test_proc::wrap(dir.path(), "server-shim", &program);

    let mut started = Command::new(&wrapper)
        .args(["a", "b"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let wrapper_pid = started.id();
    let status = started.wait().unwrap();
    let mut out = String::new();
    std::io::Read::read_to_string(&mut started.stdout.take().unwrap(), &mut out).unwrap();

    assert_eq!(status.code(), Some(4));
    assert_eq!(out.trim(), "out a b");
    let pid: u32 = std::fs::read_to_string(&pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_ne!(pid, wrapper_pid, "the program is the wrapper's child");
}
