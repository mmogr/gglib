//! `gglib q` whose model could not load, against a stand-in daemon: the
//! reason is the last thing it says, and it exits non-zero.
//!
//! The daemon is a stand-in in this process, bound before `gglib` runs, so
//! the command finds it on its port and launches no daemon of its own.

use std::io::Write as _;
use std::net::TcpListener;
use std::process::Stdio;

use gglib_core::domain::agent::AgentEvent;
use gglib_core::domain::runs::{RunError, RunFrames, RunInfo, RunKind, RunStatus};
use gglib_core::utils::process::cmd;
use gglib_test_http::blocking;
use serde_json::json;

#[path = "support/library.rs"]
mod library;

/// What the stand-in's daemon says of a model that could not load. The
/// command only echoes it.
const SENTENCE: &str = "The model 'qwen.Q8_0' could not load: the file is not a GGUF model \
                        (invalid magic characters).";

/// Run `id` as the stand-in reports it, in `status`.
fn run(id: &str, status: RunStatus, error: Option<RunError>) -> RunInfo {
    RunInfo {
        id: id.to_owned(),
        kind: RunKind::Agent,
        status,
        model: Some("qwen.Q8_0".to_owned()),
        device: None,
        created_at_ms: 1,
        finished_at_ms: None,
        conversation_id: None,
        last_seq: 0,
        error,
        frames: RunFrames::Agent,
    }
}

/// The run id in a request line's path: what follows `/api/runs/`, up to
/// the next `/`, `?` or space.
fn run_id(line: &str) -> String {
    let rest = line.split("/api/runs/").nth(1).unwrap_or_default();
    rest.split(['/', '?', ' '])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// What the stand-in answers `line` with: its status, type and body.
fn reply(line: &str) -> (u16, &'static str, String) {
    let id = run_id(line);
    if line.starts_with("GET /health ") {
        let health = json!({
            "service": "gglib-daemon",
            "fingerprint": gglib_build_info::FINGERPRINT,
            "debug_switches": gglib_core::debug_switches::active(),
        });
        return (200, "application/json", health.to_string());
    }
    if line.starts_with("PUT /api/runs/") {
        let queued = run(&id, RunStatus::Queued, None);
        return (201, "application/json", json!(queued).to_string());
    }
    if line.starts_with("GET /api/runs/") && line.contains("/events") {
        let waiting = serde_json::to_string(&AgentEvent::model_load_wait("qwen.Q8_0", 1_000))
            .expect("a waiting frame");
        let error = RunError {
            code: "model_unavailable".to_owned(),
            message: SENTENCE.to_owned(),
        };
        let failed = run(&id, RunStatus::Failed, Some(error));
        let sse = format!(
            "id: 1\ndata: {waiting}\n\nevent: run\ndata: {}\n\n",
            json!(failed)
        );
        return (200, "text/event-stream", sse);
    }
    (404, "application/json", "{}".to_owned())
}

/// A stand-in daemon on a loopback port, answering as [`reply`] says.
fn stand_in_daemon() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    std::thread::spawn(move || {
        // A connection that cannot be read is skipped, not the end: a
        // stand-in that stopped listening would have `gglib` launch a daemon.
        loop {
            let Ok((request, mut stream)) = blocking::accept(&listener) else {
                continue;
            };
            let (status, kind, body) = reply(request.line());
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: {kind}\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

/// A question whose model could not load says why, in the daemon's words,
/// and nothing after it: not a second line that reads as another fault.
#[test]
fn a_question_whose_model_could_not_load_ends_with_why_alone() {
    let root = tempfile::tempdir().expect("a temporary directory");
    library::library(root.path());
    let port = stand_in_daemon();

    let out = cmd(env!("CARGO_BIN_EXE_gglib"))
        .args(["q", "--quiet", "--no-tools", "--model", "qwen.Q8_0", "hi"])
        .env("GGLIB_DATA_DIR", library::data_root(root.path()))
        .env("GGLIB_RESOURCE_DIR", library::resource_root(root.path()))
        .env("GGLIB_DAEMON_PORT", port.to_string())
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("`gglib q` runs");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "it must fail\nstderr: {stderr}");
    assert!(
        stderr.contains(SENTENCE),
        "it must say why the model could not load\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("did not produce a final answer"),
        "nothing must follow why\nstderr: {stderr}"
    );
}
