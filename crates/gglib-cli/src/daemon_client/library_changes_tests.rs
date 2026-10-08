//! What the daemon is sent of a command's library changes, and when it is
//! sent nothing.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_core::events::{AppEvent, ModelSummary};
use gglib_core::ports::AppEventEmitter as _;

use super::super::auth::STAND_IN_TOKEN;
use super::super::handle_tests::daemon_health;
pub(crate) use super::super::handle_tests::nobody;
use super::super::{STAND_IN_PORT, paths};
use crate::bootstrap::{CliContext, test_context};

/// The token the daemon of a test's library has left.
pub(crate) const TOKEN: &str = "the-daemons-token";

/// `run`, with `port` for the daemon's and `token` for what the daemon that
/// serves this library has left under the data root.
pub(crate) async fn beside<T>(port: u16, token: Option<&str>, run: impl Future<Output = T>) -> T {
    let left = token.map(str::to_owned);
    STAND_IN_TOKEN
        .scope(left, STAND_IN_PORT.scope(port, run))
        .await
}

/// A request a stand-in was sent: its first line, its `Authorization` and
/// `Content-Type` headers and its body.
pub(crate) type Request = (String, Option<String>, Option<String>, String);

/// Each request a stand-in was sent.
pub(crate) type Asked = Arc<Mutex<Vec<Request>>>;

/// A server on a loopback port that answers `/health` with `health` and any
/// other request with the status `answer` and no body, and keeps what it
/// was sent.
fn stand_in(health: String, answer: &'static str) -> (u16, Asked) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let asked = Asked::default();
    let seen = Arc::clone(&asked);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            let header = |wanted: &str| {
                head.lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
                    .map(|(_, value)| value.trim().to_owned())
            };
            let length = header("content-length").and_then(|n| n.parse().ok());
            let mut body = vec![0_u8; length.unwrap_or(0)];
            let _ = stream.read_exact(&mut body);
            let line = head.lines().next().unwrap_or_default().to_owned();
            let reply = if line.starts_with("GET /health ") {
                format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{health}",
                    health.len()
                )
            } else {
                format!("HTTP/1.1 {answer}\r\nconnection: close\r\n\r\n")
            };
            let body = String::from_utf8_lossy(&body).into_owned();
            let sent = (line, header("authorization"), header("content-type"), body);
            seen.lock().unwrap().push(sent);
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    (port, asked)
}

/// A stand-in for this build's daemon, which answers an event with the
/// status `answer`.
pub(crate) fn daemon(answer: &'static str) -> (u16, Asked) {
    stand_in(daemon_health(), answer)
}

/// A stand-in for a program on the daemon's port that is not a gglib daemon.
pub(crate) fn another_program() -> (u16, Asked) {
    let health = r#"{"service":"something-else"}"#.to_owned();
    stand_in(health, "204 No Content")
}

/// What a daemon is sent for `events` when it is told them: who it is
/// first, asked with no credential, and then each event with [`TOKEN`] and
/// under the JSON content type, without which the daemon's route refuses
/// the event.
pub(crate) fn told(events: &[AppEvent]) -> Vec<Request> {
    let probe = (
        format!("GET {} HTTP/1.1", paths::HEALTH_PATH),
        None,
        None,
        String::new(),
    );
    let posts = events.iter().map(|event| {
        (
            format!("POST {} HTTP/1.1", paths::EVENTS_PATH),
            Some(format!("Bearer {TOKEN}")),
            Some("application/json".to_owned()),
            serde_json::to_string(event).expect("an event is JSON"),
        )
    });
    std::iter::once(probe).chain(posts).collect()
}

/// A command's context. Telling the daemon reads the token under the data
/// root, and naming that file makes the root's directory: this binary's own.
async fn context(dir: &tempfile::TempDir) -> CliContext {
    gglib_core::paths::isolate_data_root();
    test_context(dir.path()).await
}

fn renamed() -> AppEvent {
    let model = ModelSummary::new(7, "Renamed", "/models/qwen.gguf", None, None);
    AppEvent::model_updated(model)
}

/// The daemon is asked who it is and then posted each event, in the order
/// `ModelOps` emitted them, with the token and as JSON. An event it has been
/// told is not kept to be told again.
#[tokio::test]
async fn the_daemon_is_posted_each_event_once_in_order_with_its_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir).await;
    let (port, asked) = daemon("204 No Content");
    let events = [renamed(), AppEvent::model_removed(7)];
    for event in &events {
        ctx.library_changes.emit(event.clone());
    }

    beside(port, Some(TOKEN), ctx.library_changes.tell_daemon(&ctx)).await;
    assert_eq!(*asked.lock().unwrap(), told(&events));

    beside(port, Some(TOKEN), ctx.library_changes.tell_daemon(&ctx)).await;
    assert_eq!(asked.lock().unwrap().len(), told(&events).len());
}

/// A command that changed nothing asks nothing, and neither does one whose
/// data root holds no token: the daemon on the port is not this library's.
#[tokio::test]
async fn nothing_is_asked_of_the_port_with_no_change_to_tell_or_no_token() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir).await;
    let (port, asked) = daemon("204 No Content");

    beside(port, Some(TOKEN), ctx.library_changes.tell_daemon(&ctx)).await;

    ctx.library_changes.emit(renamed());
    beside(port, None, ctx.library_changes.tell_daemon(&ctx)).await;

    assert_eq!(*asked.lock().unwrap(), []);
}

/// A program that is not a gglib daemon is asked who it is, and is sent
/// neither the event nor the token.
#[tokio::test]
async fn a_port_another_program_holds_is_sent_no_event() {
    let (port, asked) = another_program();
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir).await;
    ctx.library_changes.emit(renamed());

    beside(port, Some(TOKEN), ctx.library_changes.tell_daemon(&ctx)).await;

    assert_eq!(*asked.lock().unwrap(), told(&[]));
}

/// With nothing on the daemon's port the events are dropped, as they were
/// before a daemon could be told: a daemon that comes up later is not sent
/// them.
#[tokio::test]
async fn with_no_daemon_running_the_events_are_dropped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir).await;
    ctx.library_changes.emit(renamed());

    beside(nobody(), Some(TOKEN), ctx.library_changes.tell_daemon(&ctx)).await;

    let (port, asked) = daemon("204 No Content");
    beside(port, Some(TOKEN), ctx.library_changes.tell_daemon(&ctx)).await;
    assert_eq!(*asked.lock().unwrap(), []);
}

/// A daemon that refuses an event, as one on another data root refuses the
/// token and an older build has no route, is sent no more of them.
#[tokio::test]
async fn a_daemon_that_refuses_an_event_is_sent_no_more() {
    for answer in ["401 Unauthorized", "405 Method Not Allowed"] {
        let (port, asked) = daemon(answer);
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = context(&dir).await;
        ctx.library_changes.emit(renamed());
        ctx.library_changes.emit(AppEvent::model_removed(7));

        beside(port, Some(TOKEN), ctx.library_changes.tell_daemon(&ctx)).await;

        assert_eq!(*asked.lock().unwrap(), told(&[renamed()]), "{answer}");
    }
}
