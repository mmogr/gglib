//! A bounded run, against a stand-in program that answers, fails or never
//! exits.

use std::time::{Duration, Instant};

use super::{super::cmd, run_bounded};

#[test]
fn a_program_that_answers_gives_its_output() {
    let dir = tempfile::tempdir().unwrap();
    let program = gglib_test_proc::install(
        dir.path(),
        "tool",
        "print tool 1.2.3\neprint a note\nexit 0",
    );

    let output = run_bounded(cmd(&program).arg("--version"), Duration::from_secs(10)).unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "tool 1.2.3");
    assert_eq!(String::from_utf8_lossy(&output.stderr).trim(), "a note");
}

#[test]
fn a_program_that_fails_gives_its_status() {
    let dir = tempfile::tempdir().unwrap();
    let program = gglib_test_proc::install(dir.path(), "tool", "eprint no such flag\nexit 3");

    let output = run_bounded(&mut cmd(&program), Duration::from_secs(10)).unwrap();

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr).trim(),
        "no such flag"
    );
}

#[test]
fn a_program_that_never_exits_is_stopped_when_the_wait_is_up() {
    let dir = tempfile::tempdir().unwrap();
    let program = gglib_test_proc::install(dir.path(), "tool", "print started\nsleep 60");
    let wait = Duration::from_millis(500);

    let started = Instant::now();
    let refused = run_bounded(&mut cmd(&program), wait).expect_err("it never exits");

    assert_eq!(refused.kind(), std::io::ErrorKind::TimedOut, "{refused}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "waited {:?}",
        started.elapsed()
    );
}

#[test]
fn a_program_that_cannot_start_says_why() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-such-tool");

    let refused =
        run_bounded(&mut cmd(&missing), Duration::from_secs(1)).expect_err("nothing to run");

    assert_eq!(refused.kind(), std::io::ErrorKind::NotFound, "{refused}");
}
