//! The configure and compile steps, run against the stand-in program in
//! place of cmake.

use super::{run_compile, run_configure};
use crate::llama::build_events::BuildEvent;
use gglib_core::utils::process::cmd;
use std::process::Command;
use std::time::Duration;
use tokio::sync::mpsc;

/// The command a step is handed in place of cmake: a program that follows
/// `instructions`, with the folder it is in, which lasts as long as the
/// caller holds it.
fn stand_in(instructions: &str) -> (tempfile::TempDir, Command) {
    let dir = tempfile::tempdir().expect("a temp dir");
    let program = gglib_test_proc::install(dir.path(), "cmake", instructions);
    (dir, cmd(program))
}

/// One event as a short line, so a run reads as a list of strings.
fn shown(event: BuildEvent) -> String {
    match event {
        BuildEvent::Log { message } => format!("log {message}"),
        BuildEvent::Progress { current, total } => format!("progress {current}/{total}"),
        BuildEvent::PhaseCompleted { phase } => format!("completed {phase:?}"),
        other => format!("{other:?}"),
    }
}

/// Everything a finished step sent, in the order it sent it.
fn sent(rx: &mut mpsc::Receiver<BuildEvent>) -> Vec<String> {
    std::iter::from_fn(|| rx.try_recv().ok())
        .map(shown)
        .collect()
}

/// Take the one `line` out of `events`. The two output streams are read on
/// separate threads, so where a stderr line lands among the stdout lines is
/// not fixed, only that it is there.
fn take(events: &mut Vec<String>, line: &str) {
    let at = events
        .iter()
        .position(|event| event == line)
        .unwrap_or_else(|| panic!("no {line:?} among {events:?}"));
    events.remove(at);
}

#[tokio::test]
async fn a_step_reports_each_line_while_its_command_is_still_running() {
    let (stdin, release) = std::io::pipe().expect("a pipe");
    let (_dir, mut command) = stand_in("print from-stdout\nread\neprint from-stderr");
    command.stdin(stdin);
    let (tx, mut rx) = mpsc::channel(64);
    let step = tokio::task::spawn_blocking(move || run_configure(command, &tx));

    // The stand-in is now waiting on its stdin, so its first line can only be
    // here if the step passed it on before the command exited.
    let first = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await;
    // The stand-in reads until its stdin ends.
    drop(release);

    assert_eq!(
        first.ok().flatten().map(shown).as_deref(),
        Some("log from-stdout")
    );
    assert_eq!(
        rx.recv().await.map(shown).as_deref(),
        Some("log from-stderr")
    );
    assert_eq!(
        rx.recv().await.map(shown).as_deref(),
        Some("completed Configure")
    );
    step.await
        .expect("the step does not panic")
        .expect("the stand-in exits 0");
}

#[test]
fn a_configure_step_that_exits_nonzero_says_cmake_configuration_failed() {
    let (tx, mut rx) = mpsc::channel(64);

    // A line of nothing and a line of spaces are among what it prints.
    let (_dir, command) = stand_in("print checking\nprint\nprint    \neprint no compiler\nexit 3");

    let err = run_configure(command, &tx).expect_err("the stand-in exits 3");

    assert_eq!(err.to_string(), "CMake configuration failed");
    let mut events = sent(&mut rx);
    take(&mut events, "log no compiler");
    assert_eq!(events, ["log checking", "completed Configure"]);
}

#[test]
fn a_compile_step_that_exits_nonzero_reports_the_exit_code() {
    let (tx, mut rx) = mpsc::channel(64);

    let (_dir, command) = stand_in(
        "print [ 50%] Building CXX object a.o\nprint scanning dependencies\n\
         print [100%] Linking CXX executable x\neprint ld: fatal error\nexit 2",
    );

    let err = run_compile(command, &tx).expect_err("the stand-in exits 2");

    assert_eq!(err.to_string(), "Build failed (exit code: 2)");
    let mut events = sent(&mut rx);
    take(&mut events, "log ld: fatal error");
    assert_eq!(
        events,
        [
            "progress 50/100",
            "log [ 50%] Building CXX object a.o",
            "progress 100/100",
            "log [100%] Linking CXX executable x",
            "completed Compile",
        ]
    );
}

#[test]
fn a_step_whose_command_exits_zero_succeeds() {
    let (tx, mut rx) = mpsc::channel(64);

    let (_configure_dir, configure) = stand_in("print configured");
    let (_compile_dir, compile) = stand_in("print [3/4] Building C object b.o");

    run_configure(configure, &tx).expect("the stand-in exits 0");
    run_compile(compile, &tx).expect("the stand-in exits 0");

    assert_eq!(
        sent(&mut rx),
        [
            "log configured",
            "completed Configure",
            "progress 3/4",
            "log [3/4] Building C object b.o",
            "completed Compile",
        ]
    );
}

#[test]
fn a_step_whose_command_cannot_start_names_the_step_and_sends_nothing() {
    let (tx, mut rx) = mpsc::channel(64);
    let missing = || cmd("/nonexistent/gglib-stand-in-for-cmake");

    let configure = run_configure(missing(), &tx).expect_err("nothing to run");
    let compile = run_compile(missing(), &tx).expect_err("nothing to run");

    assert_eq!(configure.to_string(), "Failed to run CMake");
    assert_eq!(compile.to_string(), "Failed to run build");
    assert_eq!(sent(&mut rx), Vec::<String>::new());
}
