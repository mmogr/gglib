//! Tests for the key a command sends: the proxy's key as it is stored, and the
//! handle that carries the daemon's token to every call.
//!
//! How the token is read is `auth_tests.rs`; here it is that it reaches the
//! wire, and that the stored key never does.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_core::Settings;
use gglib_test_http::blocking;

use super::auth::STAND_IN_TOKEN;
use super::*;
use crate::bootstrap::{CliContext, test_context};

/// The CLI's context over `dir`'s database, with `key` stored as the proxy's.
async fn context(dir: &tempfile::TempDir, key: Option<&str>) -> CliContext {
    // The credential names the daemon's token file, and naming it makes the
    // data root's directory: this binary's own, not the checkout's.
    gglib_core::paths::isolate_data_root();
    let ctx = test_context(dir.path()).await;
    ctx.settings_repo
        .modify(&|settings: &mut Settings| {
            settings.proxy_api_key = key.map(str::to_owned);
            Ok(())
        })
        .await
        .expect("the key is stored");
    ctx
}

/// Each request a stand-in was sent: its first line and its `Authorization`
/// header.
pub(crate) type Asked = Arc<Mutex<Vec<(String, Option<String>)>>>;

/// A server on a loopback port that answers `/health` with `health` and any
/// other request with `{}`, and keeps what it was asked.
fn stand_in(health: String) -> (u16, Asked) {
    answering(health, "{}".to_owned())
}

/// [`stand_in`], answering any request but `/health` with `rest`.
pub(crate) fn answering(health: String, rest: String) -> (u16, Asked) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let asked = Asked::default();
    let seen = Arc::clone(&asked);
    std::thread::spawn(move || {
        loop {
            let Ok(taken) = blocking::incoming(&listener) else {
                continue;
            };
            // A connection that sends no whole request was still asked.
            let Ok((request, mut stream)) = taken.request() else {
                seen.lock().unwrap().push((String::new(), None));
                continue;
            };
            let line = request.line().to_owned();
            let authorization = request.header("authorization").map(str::to_owned);
            let body = if line.starts_with("GET /health ") {
                &health
            } else {
                &rest
            };
            seen.lock().unwrap().push((line, authorization));
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, asked)
}

/// What this build's daemon answers `/health` with, as far as the probe reads.
pub(crate) fn daemon_health() -> String {
    serde_json::json!({
        "service": "gglib-daemon",
        "fingerprint": gglib_build_info::FINGERPRINT,
        "debug_switches": gglib_core::debug_switches::active(),
    })
    .to_string()
}

/// A loopback port nothing listens on.
pub(crate) fn nobody() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a loopback port")
        .port()
}

/// The proxy's key is the flag when one is given, and otherwise the stored
/// key: when there is one, and it is not blank.
#[tokio::test]
async fn the_proxy_key_is_the_flag_and_then_the_stored_key_when_it_is_not_blank() {
    let flag = || Some("the-flag".to_owned());
    for (stored, given, key) in [
        (Some("the-stored-key"), None, Some("the-stored-key")),
        (Some(" the-stored-key \n"), None, Some("the-stored-key")),
        (Some("the-stored-key"), flag(), Some("the-flag")),
        (None, flag(), Some("the-flag")),
        (None, None, None),
        (Some(""), None, None),
        (Some("  \t"), None, None),
        (Some("  \t"), flag(), Some("the-flag")),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = context(&dir, stored).await;

        let sent = auth::proxy_key(&ctx, given.clone()).await;

        assert_eq!(sent.as_deref(), key, "stored {stored:?}, flag {given:?}");
    }
}

/// The daemon is presented its token and never the stored key: with no token
/// to read, nothing is sent.
#[tokio::test]
async fn the_daemon_is_presented_its_token_and_never_the_stored_key() {
    let dir = tempfile::tempdir().expect("tempdir");
    let _ctx = context(&dir, Some("the-stored-key")).await;
    let client = gglib_proxy::loopback::client;

    let none = STAND_IN_TOKEN.scope(None, async { DaemonHandle::new(client()) });
    let some = STAND_IN_TOKEN.scope(Some(TOKEN.to_owned()), async {
        DaemonHandle::new(client())
    });

    assert_eq!(none.await.api_key, None);
    assert_eq!(some.await.api_key.as_deref(), Some(TOKEN));
}

/// The token a test's daemon has left.
const TOKEN: &str = "the-daemons-token";

/// `run`, with `port` for the daemon's and [`TOKEN`] left under the data root.
async fn beside<T>(port: u16, run: impl Future<Output = T>) -> T {
    STAND_IN_TOKEN
        .scope(Some(TOKEN.to_owned()), STAND_IN_PORT.scope(port, run))
        .await
}

/// A command that asks for the running daemon is handed one that carries the
/// token, and a call made through it sends that token.
#[tokio::test]
async fn a_running_daemon_is_handed_back_carrying_the_token_every_call_sends() {
    gglib_core::paths::isolate_data_root();
    let (port, asked) = stand_in(daemon_health());

    let Ok(handle) = beside(port, running()).await else {
        panic!("the stand-in answers as a daemon");
    };
    assert_eq!(handle.api_key.as_deref(), Some(TOKEN));
    beside(port, handle.setup_status())
        .await
        .expect("the stand-in answers");

    let last = asked.lock().unwrap().last().cloned();
    assert_eq!(
        last,
        Some((
            format!("GET {} HTTP/1.1", paths::SETUP_STATUS_PATH),
            Some(format!("Bearer {TOKEN}"))
        ))
    );
}

/// A command that needs the daemon is handed the one already running, with
/// its token, and launches nothing.
#[tokio::test]
async fn a_command_that_needs_the_daemon_is_handed_the_running_one_with_its_token() {
    gglib_core::paths::isolate_data_root();
    let (port, asked) = stand_in(daemon_health());

    let handle = beside(port, ensure_daemon())
        .await
        .expect("the stand-in answers as a daemon");

    assert_eq!(handle.api_key.as_deref(), Some(TOKEN));
    assert_eq!(asked.lock().unwrap().len(), 1, "one probe, and no wait");
}

/// A port another program holds is an error that says so, and nothing is
/// launched at it; a port nobody holds is where a daemon is launched, which
/// a test stops short of.
#[tokio::test]
async fn a_command_that_needs_the_daemon_launches_one_only_when_nothing_holds_the_port() {
    gglib_core::paths::isolate_data_root();
    let (foreign, _) = stand_in(r#"{"service":"something-else"}"#.to_owned());

    let held = STAND_IN_PORT.scope(foreign, ensure_daemon()).await;
    let free = STAND_IN_PORT.scope(nobody(), ensure_daemon()).await;

    let held = format!("{:#}", held.err().expect("another program holds the port"));
    assert!(held.contains("in use by another program"), "{held}");
    assert!(!held.contains("launch"), "{held}");
    let free = format!("{:#}", free.err().expect("a test launches no daemon"));
    assert!(free.contains("could not launch the gglib daemon"), "{free}");
}

/// With nothing on the daemon's port, or something that is not a gglib
/// daemon, there is no handle, and the caller is told which it was.
#[tokio::test]
async fn without_a_daemon_running_says_what_is_there_instead() {
    gglib_core::paths::isolate_data_root();
    let (foreign, _) = stand_in(r#"{"service":"something-else"}"#.to_owned());

    let nothing = STAND_IN_PORT.scope(nobody(), running()).await;
    let another = STAND_IN_PORT.scope(foreign, running()).await;

    assert_eq!(nothing.err(), Some(Absent::NotRunning));
    assert_eq!(another.err(), Some(Absent::ForeignServer));
}
