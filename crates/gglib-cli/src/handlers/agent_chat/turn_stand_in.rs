//! A stand-in for the daemon's door, for the tests of a turn and of the
//! REPL: it answers a turn's `PUT` as it is told, the run's events as the
//! frames it is given and the run's end, a cancel as done, and a chat's
//! change as it is told, and keeps every request.

use std::fmt::Write as _;
use std::io::Write as _;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_core::domain::branching::ChatChanged;
use gglib_core::domain::runs::{RunError, RunFrames, RunInfo, RunKind, RunStatus};
use gglib_test_http::blocking;
use serde_json::{Value, json};

use crate::daemon_client::DaemonHandle;
use crate::daemon_client::handle_tests::daemon_health;

/// Each request a stand-in took: its first line and its body.
pub(crate) type Taken = Arc<Mutex<Vec<(String, String)>>>;

/// How the stand-in answers a turn's `PUT`.
#[derive(Clone)]
pub(crate) enum Answer {
    /// It runs the turn: its events are `frames`, then the run's end in
    /// `status`, with `error` when it failed.
    Runs {
        frames: Vec<String>,
        status: RunStatus,
        error: Option<RunError>,
    },
    /// It refuses the turn with this status and coded body.
    Refuses(u16, Value),
    /// It runs the turn, and its events are `frames` with no end after
    /// them: the stream closes before the run ends.
    Cut(Vec<String>),
    /// It runs the turn, and its events cannot be read.
    Unreadable,
    /// It changes a chat, answering what [`ChatChanged`] says, or refuses
    /// the change with this status and coded body; and runs a turn, which
    /// answers `Done.`
    Changes(Result<ChatChanged, (u16, Value)>),
}

/// A run as the stand-in reports it.
pub(crate) fn run(id: &str, status: RunStatus, error: Option<RunError>) -> RunInfo {
    RunInfo {
        id: id.to_owned(),
        kind: RunKind::Agent,
        status,
        model: Some("qwen".to_owned()),
        device: None,
        created_at_ms: 1,
        finished_at_ms: None,
        conversation_id: Some(7),
        last_seq: 0,
        error,
        frames: RunFrames::Agent,
    }
}

/// The run id in a request line's path: what follows `/api/runs/`, up to
/// the next `/`, `?` or space.
pub(crate) fn run_id(line: &str) -> String {
    let rest = line.split("/api/runs/").nth(1).unwrap_or_default();
    rest.split(['/', '?', ' '])
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// What the stand-in answers `line` with: its status, type and body.
fn reply(line: &str, answer: &Answer) -> (u16, &'static str, String) {
    let id = run_id(line);
    if line.starts_with("GET /health ") {
        return (200, "application/json", daemon_health());
    }
    if line.starts_with("GET /api/runs ") {
        return (200, "application/json", r#"{"runs":[]}"#.to_owned());
    }
    if line.contains("/cancel") {
        let info = run(&id, RunStatus::Cancelled, None);
        return (200, "application/json", json!(info).to_string());
    }
    match answer {
        Answer::Changes(changed) if line.contains("/changes") => match changed {
            Ok(changed) => (200, "application/json", json!(changed).to_string()),
            Err((status, body)) => (*status, "application/json", body.to_string()),
        },
        Answer::Cut(frames) if line.contains("/events") => {
            let mut sse = String::new();
            for (n, frame) in frames.iter().enumerate() {
                let _ = write!(sse, "id: {}\ndata: {frame}\n\n", n + 1);
            }
            (200, "text/event-stream", sse)
        }
        Answer::Changes(_) if line.contains("/events") => {
            let done = json!({"type": "final_answer", "content": "Done."});
            let end = run(&id, RunStatus::Completed, None);
            let sse = format!(
                "id: 1\ndata: {done}\n\nevent: run\ndata: {}\n\n",
                json!(end)
            );
            (200, "text/event-stream", sse)
        }
        Answer::Unreadable if line.contains("/events") => {
            (500, "application/json", r#"{"error":"boom"}"#.to_owned())
        }
        Answer::Refuses(status, body) if line.starts_with("PUT ") => {
            (*status, "application/json", body.to_string())
        }
        Answer::Runs { .. } | Answer::Cut(_) | Answer::Unreadable | Answer::Changes(_)
            if line.starts_with("PUT ") =>
        {
            let info = run(&id, RunStatus::Queued, None);
            (201, "application/json", json!(info).to_string())
        }
        Answer::Runs {
            frames,
            status,
            error,
        } if line.contains("/events") => {
            let mut sse = String::new();
            for (n, frame) in frames.iter().enumerate() {
                let _ = write!(sse, "id: {}\ndata: {frame}\n\n", n + 1);
            }
            let end = run(&id, *status, error.clone());
            let _ = write!(sse, "event: run\ndata: {}\n\n", json!(end));
            (200, "text/event-stream", sse)
        }
        _ => (404, "application/json", "{}".to_owned()),
    }
}

/// A stand-in daemon on a loopback port: `/health` as this build's daemon,
/// its runs listed as none, a turn's `PUT` answered as `answer` says, the
/// run's events as its frames and its end, and a cancel as done. Keeps
/// every request.
pub(crate) fn stand_in(answer: Answer) -> (u16, Taken) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let taken = Taken::default();
    let kept = Arc::clone(&taken);
    std::thread::spawn(move || {
        loop {
            let Ok((request, mut stream)) = blocking::accept(&listener) else {
                continue;
            };
            let line = request.line().to_owned();
            let body = request.body_text();
            kept.lock().unwrap().push((line.clone(), body));
            let (status, kind, body) = reply(&line, &answer);
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-type: {kind}\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, taken)
}

/// A handle on the stand-in, carrying no credential.
pub(crate) fn handle() -> DaemonHandle {
    DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    }
}
