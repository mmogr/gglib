//! `gglib model remove` against a model that is being served, run as a
//! person runs it: the built binary, over a library on disk.
//!
//! The command is a process apart from whatever serves the model, so what
//! it knows of a server is the pid file kept for it under the data root.
//! Each test has a data root of its own in a temporary directory and writes
//! the pid files there itself.
//!
//! No llama-server runs. A pid file counts while its pid is a running
//! llama-server of the data root's, which is asked of the pid's executable,
//! so the server here is `sleep`, with the root's llama-server path linked
//! to it.
//!
//! Unix-only for that link and for `sleep`, not for the behaviour.

#![cfg(unix)]

#[path = "support/library.rs"]
mod library;

use std::path::Path;
use std::process::{Child, Command, Output, Stdio};

use library::{data_root, gglib, library, resource_root, run, write_gguf};

/// The program a stand-in server runs.
const SLEEP: &str = "/bin/sleep";

/// What `gglib model remove` prints when it removes the library's one model
/// without asking.
const REMOVED: &str = "✓ Model 'qwen.Q8_0' (ID 1) successfully removed from database.\n";

/// What it says instead when that model is being served on port 9001.
const REFUSED: &str = "Model 'qwen.Q8_0' (ID 1) is being served by a llama-server on port 9001, \
     so it was not removed.\n\
     Stop it first, in the gglib app or with `gglib daemon stop` (which stops \
     the daemon and every model it is serving), then remove it.";

/// A running process that `gglib` takes for a llama-server of any root
/// [`takes_sleep_for_llama_server`] was called on. Stopped when dropped.
struct StandIn(Child);

impl StandIn {
    fn start() -> Self {
        Self(
            Command::new(SLEEP)
                .arg("120")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("sleep starts"),
        )
    }

    fn pid(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Link the llama-server path of the library in `root` to `sleep`, so a
/// running `sleep` is, to `gglib` there, a running llama-server of its own.
fn takes_sleep_for_llama_server(root: &Path) {
    let bin = resource_root(root).join(".llama").join("bin");
    std::fs::create_dir_all(&bin).expect("the bin directory");
    let sleep = std::fs::canonicalize(SLEEP).expect("sleep is installed");
    std::os::unix::fs::symlink(sleep, bin.join("llama-server")).expect("the link");
}

/// Record under `root`, as whatever starts a server records it, that model
/// 1 is served by `pid` on port 9001: a file named for the model, holding
/// the pid and then the port.
fn record_a_server(root: &Path, pid: u32) {
    let pids = data_root(root).join("pids");
    std::fs::create_dir_all(&pids).expect("the pid directory");
    std::fs::write(pids.join("1.pid"), format!("{pid}\n9001\n")).expect("the pid file");
}

/// The pid of a process that has run and been reaped, so it names nothing.
fn pid_of_a_process_that_is_gone() -> u32 {
    let mut child = Command::new(SLEEP).arg("0").spawn().expect("sleep starts");
    let pid = child.id();
    child.wait().expect("it ends");
    pid
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Whether the library in `root` still holds model 1.
fn holds_the_model(root: &Path) -> bool {
    run(root, &["model", "inspect", "1"], "").status.success()
}

/// Asked or not: `--force` skips the question, and does not stop a server
/// or remove a model from under one. Nothing is asked either way, since a
/// removal that would be refused is refused first.
#[test]
fn a_model_served_under_this_data_root_is_refused_and_told_how_to_stop_it() {
    let root = tempfile::tempdir().expect("temp data dir");
    let weights = library(root.path());
    takes_sleep_for_llama_server(root.path());
    let server = StandIn::start();
    record_a_server(root.path(), server.pid());

    for args in [
        &["model", "remove", "1", "--force"][..],
        &["model", "remove", "1"],
        &["model", "remove", "qwen.Q8_0"],
    ] {
        let out = run(root.path(), args, "");

        assert_eq!(out.status.code(), Some(1), "{args:?}: {}", stderr(&out));
        assert!(stderr(&out).contains(REFUSED), "{args:?}: {}", stderr(&out));
        assert_eq!(stdout(&out), "", "{args:?}");
        assert!(weights.exists(), "{args:?}: the refusal took the file");
    }

    // The row is as it was, and `inspect` reads the same record.
    let inspected = gglib(root.path(), &["model", "inspect", "1"], "");
    assert!(
        inspected.contains("  Serving        : yes (port 9001)\n"),
        "{inspected}"
    );

    // Stopped, it is removed as any model is.
    drop(server);
    let removed = gglib(root.path(), &["model", "remove", "1", "--force"], "");
    assert_eq!(removed, REMOVED);
    assert!(!holds_the_model(root.path()));
}

/// A server is one model's. Another model in the same library is removed
/// as it is when nothing is being served.
#[test]
fn a_server_for_one_model_does_not_block_the_removal_of_another() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    let second = write_gguf(root.path(), "phi.Q4_0.gguf");
    gglib(
        root.path(),
        &["model", "add", second.to_str().unwrap()],
        "4\n",
    );
    takes_sleep_for_llama_server(root.path());
    let server = StandIn::start();
    record_a_server(root.path(), server.pid());

    let removed = gglib(root.path(), &["model", "remove", "2", "--force"], "");
    let refused = run(root.path(), &["model", "remove", "1", "--force"], "");

    assert_eq!(
        removed,
        "✓ Model 'phi.Q4_0' (ID 2) successfully removed from database.\n"
    );
    assert!(stderr(&refused).contains(REFUSED), "{}", stderr(&refused));
    assert!(holds_the_model(root.path()));
}

/// A pid file outlives a server that was killed. Its process is gone, and
/// the model is removed as one that is not served.
#[test]
fn a_record_whose_process_is_gone_does_not_block_a_removal() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    takes_sleep_for_llama_server(root.path());
    record_a_server(root.path(), pid_of_a_process_that_is_gone());

    let removed = gglib(root.path(), &["model", "remove", "1", "--force"], "");

    assert_eq!(removed, REMOVED);
    assert!(!holds_the_model(root.path()));
}

/// A stale record's pid can since have gone to anything. This test's own
/// process is running, and is not the root's llama-server.
#[test]
fn a_record_whose_pid_is_another_program_does_not_block_a_removal() {
    let root = tempfile::tempdir().expect("temp data dir");
    library(root.path());
    takes_sleep_for_llama_server(root.path());
    record_a_server(root.path(), std::process::id());

    let removed = gglib(root.path(), &["model", "remove", "1", "--force"], "");

    assert_eq!(removed, REMOVED);
    assert!(!holds_the_model(root.path()));
}

/// Two data roots, each with a model 1, and one server, recorded under
/// `there`. Both take it for a llama-server of their own, so only where it
/// is recorded tells them apart: `here` removes its model 1, and `there`
/// refuses to remove its own.
#[test]
fn a_server_recorded_under_another_data_root_does_not_block_a_removal_here() {
    let here = tempfile::tempdir().expect("temp data dir");
    let there = tempfile::tempdir().expect("temp data dir");
    for root in [here.path(), there.path()] {
        library(root);
        takes_sleep_for_llama_server(root);
    }
    let server = StandIn::start();
    record_a_server(there.path(), server.pid());

    let removed = gglib(here.path(), &["model", "remove", "1", "--force"], "");
    let refused = run(there.path(), &["model", "remove", "1", "--force"], "");

    assert_eq!(removed, REMOVED);
    assert!(!holds_the_model(here.path()));
    assert!(stderr(&refused).contains(REFUSED), "{}", stderr(&refused));
    assert!(holds_the_model(there.path()));
}
