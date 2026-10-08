//! Tests for the key a command sends: the proxy's key as it is stored, and the
//! handle that carries the daemon's credential to every call.
//!
//! Which of the token, the operator's key and the stored key the daemon is
//! presented is `auth_tests.rs`; here it is that the one resolution reaches
//! the wire.

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_core::Settings;

use super::*;
use crate::bootstrap::test_context;

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
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            let line = head.lines().next().unwrap_or_default().to_owned();
            let authorization = head
                .lines()
                .filter_map(|line| line.split_once(':'))
                .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                .map(|(_, value)| value.trim().to_owned());
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

/// With no token to read and no operator's key, the daemon is presented the
/// stored key, and nothing when that is blank: the rule every command's
/// handle is built by.
#[tokio::test]
async fn with_no_token_and_no_operators_key_the_daemon_is_presented_the_stored_key() {
    let nothing_else = auth::Local {
        env: None,
        token_path: None,
    };
    for (stored, presented) in [
        (Some("the-stored-key"), Some("the-stored-key")),
        (Some(" "), None),
        (None, None),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let ctx = context(&dir, stored).await;

        let sent = nothing_else.credential(auth::proxy_key(&ctx, None)).await;

        assert_eq!(sent.as_deref(), presented, "stored {stored:?}");
    }
}

/// A command that asks for the running daemon is handed one that carries the
/// credential, and a call made through it sends that credential.
#[tokio::test]
async fn a_running_daemon_is_handed_back_carrying_the_credential_every_call_sends() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Some("the-stored-key")).await;
    let presented = auth::daemon_api_key(&ctx).await;
    assert!(presented.is_some(), "a key is stored, so one is presented");
    let (port, asked) = stand_in(daemon_health());

    let Ok(handle) = STAND_IN_PORT.scope(port, running(&ctx)).await else {
        panic!("the stand-in answers as a daemon");
    };
    assert_eq!(handle.api_key, presented);
    STAND_IN_PORT
        .scope(port, handle.setup_status())
        .await
        .expect("the stand-in answers");

    let last = asked.lock().unwrap().last().cloned();
    let bearer = presented.map(|key| format!("Bearer {key}"));
    assert_eq!(
        last,
        Some((format!("GET {} HTTP/1.1", paths::SETUP_STATUS_PATH), bearer))
    );
}

/// A command that needs the daemon is handed the one already running, with
/// the same credential, and launches nothing.
#[tokio::test]
async fn a_command_that_needs_the_daemon_is_handed_the_running_one_with_the_credential() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Some("the-stored-key")).await;
    let presented = auth::daemon_api_key(&ctx).await;
    assert!(presented.is_some(), "a key is stored, so one is presented");
    let (port, asked) = stand_in(daemon_health());

    let handle = STAND_IN_PORT
        .scope(port, ensure_daemon(&ctx))
        .await
        .expect("the stand-in answers as a daemon");

    assert_eq!(handle.api_key, presented);
    assert_eq!(asked.lock().unwrap().len(), 1, "one probe, and no wait");
}

/// A port another program holds is an error that says so, and nothing is
/// launched at it; a port nobody holds is where a daemon is launched, which
/// a test stops short of.
#[tokio::test]
async fn a_command_that_needs_the_daemon_launches_one_only_when_nothing_holds_the_port() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, None).await;
    let (foreign, _) = stand_in(r#"{"service":"something-else"}"#.to_owned());

    let held = STAND_IN_PORT.scope(foreign, ensure_daemon(&ctx)).await;
    let free = STAND_IN_PORT.scope(nobody(), ensure_daemon(&ctx)).await;

    let held = format!("{:#}", held.err().expect("another program holds the port"));
    assert!(held.contains("in use by another program"), "{held}");
    assert!(!held.contains("launch"), "{held}");
    let free = format!("{:#}", free.err().expect("a test launches no daemon"));
    assert!(free.contains("could not launch the gglib daemon"), "{free}");
}

/// A handle built without asking the daemon anything carries the credential
/// too: `remote disable` and the model list's summary probe for themselves.
#[tokio::test]
async fn a_handle_built_without_a_probe_carries_the_credential() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Some("the-stored-key")).await;
    let presented = auth::daemon_api_key(&ctx).await;
    assert!(presented.is_some(), "a key is stored, so one is presented");

    let handle = DaemonHandle::new(&ctx, gglib_proxy::loopback::client()).await;

    assert_eq!(handle.api_key, presented);
}

/// With nothing on the daemon's port, or something that is not a gglib
/// daemon, there is no handle, and the caller is told which it was.
#[tokio::test]
async fn without_a_daemon_running_says_what_is_there_instead() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = context(&dir, Some("the-stored-key")).await;
    let (foreign, _) = stand_in(r#"{"service":"something-else"}"#.to_owned());

    let nothing = STAND_IN_PORT.scope(nobody(), running(&ctx)).await;
    let another = STAND_IN_PORT.scope(foreign, running(&ctx)).await;

    assert_eq!(nothing.err(), Some(Absent::NotRunning));
    assert_eq!(another.err(), Some(Absent::ForeignServer));
}
