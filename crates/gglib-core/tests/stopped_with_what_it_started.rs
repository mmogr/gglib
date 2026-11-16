//! A child that is stopped takes what it started with it (#2012).
//!
//! An MCP server whose command is a shim is two processes: on Windows
//! `npx.cmd` is `cmd.exe` running `node.exe`, and on macOS and Linux a
//! wrapper script is `sh` running the program it names. Stopping the server
//! ended only the first, and the server behind it kept running.
//!
//! The test starts a stand-in server through such a wrapper
//! ([`gglib_test_proc::wrap`]), as the MCP client starts a server, waits
//! until the server behind it runs, and stops it as the MCP client stops
//! one. The server writes [`READY`] to its output, a pipe the test reads,
//! and sleeps. The pipe ends only when every process holding it has ended,
//! so the end of the output is the last of them ending: it must come within
//! [`GONE`] of the stop. It does this [`CYCLES`] times, so a wrapper that
//! starts its program before it is held whole would show.
//!
//! If the server behind the wrapper outlives the stop, it ends by itself
//! after [`LIFE`]; on Windows the test's runtime waits for it before the
//! failure is reported, since tokio reads a Windows pipe on a thread of its
//! own.

use std::process::Stdio;
use std::time::Duration;

use gglib_core::utils::process::async_cmd;
use gglib_core::utils::process::tree::{self, Tree};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{ChildStdout, Command};

/// What the server behind the wrapper prints once it runs.
const READY: &str = "server ready";
/// How long the server lives unless something ends it.
const LIFE: Duration = Duration::from_secs(20);
/// How long the test waits for the server to start.
const START: Duration = Duration::from_secs(20);
/// How soon every process must be gone after the stop.
const GONE: Duration = Duration::from_secs(2);
/// How many times the server is started and stopped.
const CYCLES: usize = 10;

#[tokio::test]
async fn a_server_started_through_a_wrapper_is_stopped_with_the_wrapper() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let server = gglib_test_proc::install(
        dir.path(),
        "server",
        &format!("print {READY}\nsleep {}", LIFE.as_secs()),
    );
    let wrapper = gglib_test_proc::wrap(dir.path(), "server-shim", &server);

    for cycle in 1..=CYCLES {
        let mut command = async_cmd(&wrapper);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let (started, stdout) = start(command);
        let mut lines = BufReader::new(stdout).lines();
        let first = tokio::time::timeout(START, lines.next_line())
            .await
            .unwrap_or_else(|_| panic!("the server had not started {}s on", START.as_secs()))
            .expect("reading the server's output");
        assert_eq!(first.as_deref(), Some(READY), "the server did not start");

        stop(started);

        let ended = tokio::time::timeout(GONE, async {
            while lines.next_line().await.is_ok_and(|line| line.is_some()) {}
        })
        .await;
        assert!(
            ended.is_ok(),
            "start {cycle}: the server behind the wrapper was still running {}s after it was \
             stopped",
            GONE.as_secs()
        );
    }
}

/// A server started as the MCP client starts one.
struct Started(Tree);

/// Start `command` as the MCP client starts a server; its output is the
/// test's to read.
fn start(mut command: Command) -> (Started, ChildStdout) {
    let mut server = tree::spawn(&mut command).expect("starting the wrapper");
    if let Some(e) = server.loose() {
        panic!("the server could not be started as a tree: {e}");
    }
    let stdout = server.child().stdout.take().expect("the server's output");
    (Started(server), stdout)
}

/// Stop the server as the MCP client stops one.
fn stop(started: Started) {
    let mut server = started.0;
    server.stop();
}
