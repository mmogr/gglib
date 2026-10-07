//! The configure and compile steps, run against `sh` standing in for cmake.

use super::{run_compile, run_configure};
use crate::llama::build_events::BuildEvent;
use gglib_core::utils::process::cmd;
use std::io::Write;
use std::process::Command;
use std::time::Duration;
use tokio::sync::mpsc;

/// `sh -c <script>`, the command a step is handed in place of cmake.
fn stand_in(script: &str) -> Command {
    let mut command = cmd("sh");
    command.args(["-c", script]);
    command
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
    let (stdin, mut release) = std::io::pipe().expect("a pipe");
    let mut command = stand_in("echo from-stdout; read _; echo from-stderr >&2");
    command.stdin(stdin);
    let (tx, mut rx) = mpsc::channel(64);
    let step = tokio::task::spawn_blocking(move || run_configure(command, &tx));

    // The stand-in is now waiting on its stdin, so its first line can only be
    // here if the step passed it on before the command exited.
    let first = tokio::time::timeout(Duration::from_secs(30), rx.recv()).await;
    release.write_all(b"\n").expect("let the stand-in go on");
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

    let err = run_configure(
        stand_in("echo checking; echo; echo '   '; echo 'no compiler' >&2; exit 3"),
        &tx,
    )
    .expect_err("the stand-in exits 3");

    assert_eq!(err.to_string(), "CMake configuration failed");
    let mut events = sent(&mut rx);
    take(&mut events, "log no compiler");
    assert_eq!(events, ["log checking", "completed Configure"]);
}

#[test]
fn a_compile_step_that_exits_nonzero_reports_the_exit_code() {
    let (tx, mut rx) = mpsc::channel(64);

    let err = run_compile(
        stand_in(
            "echo '[ 50%] Building CXX object a.o'; echo 'scanning dependencies'; \
             echo '[100%] Linking CXX executable x'; echo 'ld: fatal error' >&2; exit 2",
        ),
        &tx,
    )
    .expect_err("the stand-in exits 2");

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

    run_configure(stand_in("echo configured"), &tx).expect("the stand-in exits 0");
    run_compile(stand_in("echo '[3/4] Building C object b.o'"), &tx).expect("the stand-in exits 0");

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
