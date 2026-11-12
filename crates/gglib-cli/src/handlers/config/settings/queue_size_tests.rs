//! What the daemon is sent when a command changes the download queue size,
//! and what the command says when it cannot be told. A stand-in on a
//! loopback port answers in the daemon's place.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use gglib_test_http::blocking;
use serde_json::{Value, json};

use super::super::reset::handle_reset;
use super::super::set::handle_set;
use super::super::set::tests::key;
use super::super::unset::handle_unset;
use super::*;
use crate::bootstrap::test_context;
use crate::daemon_client::handle_tests::{daemon_health, nobody};
use crate::daemon_client::library_changes::tests::{TOKEN, beside};

/// Each request a stand-in was sent but its `/health` probes: its first
/// line, its `Authorization` header and its body.
type Asked = Arc<Mutex<Vec<(String, Option<String>, String)>>>;

/// A stand-in daemon: `/health` as this build's daemon answers it, and any
/// other request with `status`, or, with none, read and then dropped
/// unanswered.
fn daemon(status: Option<&'static str>) -> (u16, Asked) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let asked = Asked::default();
    let seen = Arc::clone(&asked);
    let health = daemon_health();
    std::thread::spawn(move || {
        loop {
            let Ok(taken) = blocking::incoming(&listener) else {
                continue;
            };
            let Ok((request, mut stream)) = taken.request() else {
                continue;
            };
            let line = request.line().to_owned();
            let reply = if line.starts_with("GET /health ") {
                Some(("200 OK", health.as_str()))
            } else {
                let auth = request.header("authorization").map(str::to_owned);
                let sent = (line, auth, request.body_text());
                seen.lock().unwrap().push(sent);
                status.map(|status| (status, ""))
            };
            if let Some((status, body)) = reply {
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
        }
    });
    (port, asked)
}

/// The bodies of the requests a stand-in was sent, each checked to be the
/// settings write, with this data root's token.
fn settings_writes(asked: &Asked) -> Vec<Value> {
    let asked = asked.lock().unwrap();
    asked
        .iter()
        .map(|(line, auth, body)| {
            assert_eq!(line, "PUT /api/config/settings HTTP/1.1");
            assert_eq!(auth.as_deref(), Some(format!("Bearer {TOKEN}").as_str()));
            serde_json::from_str(body).expect("a JSON body")
        })
        .collect()
}

/// `set`, `unset` and `reset` each send the daemon the one field, as the
/// size now stored: the one set, none, and the default. A `set` of another
/// key sends nothing.
#[tokio::test]
async fn each_command_that_changes_the_queue_size_sends_the_daemon_that_field() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_context(dir.path()).await;
    let (port, asked) = daemon(Some("200 OK"));

    beside(port, Some(TOKEN), async {
        handle_set(&ctx, key("max-download-queue-size"), "3")
            .await
            .expect("set");
        handle_set(&ctx, key("default-context-size"), "8192")
            .await
            .expect("set");
        handle_unset(&ctx, key("max-download-queue-size"))
            .await
            .expect("unset");
        handle_reset(&ctx, true).await.expect("reset");
    })
    .await;

    let sent = settings_writes(&asked);
    let field = |size: Value| json!({ "maxDownloadQueueSize": size });
    assert_eq!(
        sent,
        [field(json!(3)), field(Value::Null), field(json!(10))]
    );
    // The daemon's route reads that body as that one field and no other.
    let read: gglib_app_services::types::UpdateSettingsRequest =
        serde_json::from_value(sent[1].clone()).unwrap();
    assert_eq!(read.max_download_queue_size, Some(None));
    assert_eq!(read.default_context_size, None);
}

/// With no daemon that serves this library nobody is asked or warned: none
/// on the port though a token is left, one on the port with no token here,
/// or one that refuses the token here (401), which serves another library.
#[tokio::test]
async fn with_no_daemon_of_this_library_nothing_is_sent_or_said() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_context(dir.path()).await;
    let (other, asked_other) = daemon(Some("200 OK"));
    let (refusing, _) = daemon(Some("401 Unauthorized"));

    for (port, token) in [
        (nobody(), Some(TOKEN)),
        (other, None),
        (refusing, Some(TOKEN)),
    ] {
        let said = beside(port, token, tell_daemon(Some(3))).await;
        assert_eq!(said, None, "{port} {token:?}");
    }
    assert!(asked_other.lock().unwrap().is_empty());
    beside(
        other,
        None,
        handle_set(&ctx, key("max-download-queue-size"), "4"),
    )
    .await
    .expect("set");
    assert!(asked_other.lock().unwrap().is_empty());
}

/// A daemon that refuses the write, or does not answer it, is a warning
/// that says when the size applies, and the command still succeeds with the
/// size stored.
#[tokio::test]
async fn a_daemon_that_is_not_told_is_a_warning_and_the_size_is_stored() {
    gglib_core::paths::isolate_data_root();
    let dir = tempfile::tempdir().unwrap();
    let ctx = test_context(dir.path()).await;

    for (status, size) in [(Some("500 Internal Server Error"), 5), (None, 6)] {
        let (port, asked) = daemon(status);
        let said = beside(port, Some(TOKEN), tell_daemon(Some(size))).await;
        let said = said.unwrap_or_else(|| panic!("{status:?}: no warning"));
        assert!(
            said.starts_with(
                "⚠ The running daemon was not told the download queue size now stored ("
            ) && said.ends_with("). It takes the stored size when it next starts."),
            "{said}"
        );

        let typed = size.to_string();
        let set = handle_set(&ctx, key("max-download-queue-size"), &typed);
        let set = beside(port, Some(TOKEN), set).await;
        set.expect("the command succeeds");
        let stored = ctx.app.settings().get().await.unwrap();
        assert_eq!(stored.max_download_queue_size, Some(size), "{status:?}");
        assert_eq!(asked.lock().unwrap().len(), 2, "{status:?}");
    }
}
