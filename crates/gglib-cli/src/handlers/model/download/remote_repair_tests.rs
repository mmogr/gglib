//! `gglib model repair`, watching the download its repair queued: what it
//! exits with, and what it says of the files when they do not come back.

use std::collections::VecDeque;
use std::io::Write as _;
use std::net::TcpListener;
use std::sync::Mutex;

use gglib_core::download::DownloadOutcome;

use super::super::monitor::tests::{COMPLETED, MINE, MINE_ID, ended, failed, running, snapshot};
use super::*;
use crate::daemon_client::STAND_IN_PORT;
use crate::handlers::agent_chat::sight::sight_tests::read_request;
use crate::handlers::model::test_library::{library, run};

/// What the daemon answers a repair that deleted `a.gguf` and `b.gguf` and
/// queued the download of `MINE`.
fn started() -> RepairStarted {
    RepairStarted {
        id: MINE_ID.to_owned(),
        files: vec!["a.gguf".to_owned(), "b.gguf".to_owned()],
    }
}

/// What is said of `files` when the download of `MINE` did not bring them
/// back.
fn missing(files: &str) -> String {
    format!(
        "Missing from the model's folder: {files}. Run `gglib model download owner/mine \
         --quantization Q8_0` to fetch what is missing."
    )
}

/// Repair with a request the daemon answers `answer`, for a model whose
/// files are in `folder`, then watch over a queue that reads as each of
/// `snapshots` in turn, never detaching. Answers the result and how many
/// snapshots were left unread.
async fn repair_and_watch(
    folder: &Path,
    answer: Result<RepairStarted>,
    snapshots: Vec<QueueSnapshot>,
) -> (Result<()>, usize) {
    let script = Mutex::new(VecDeque::from(snapshots));
    let console = Arc::new(CliConsole::hidden());
    let poll = || async {
        let next = script.lock().unwrap().pop_front();
        next.context("read past the last snapshot")
    };

    let never = std::future::pending();
    let repair = async { answer };
    let result = repair_then_watch(console, folder, repair, Duration::ZERO, poll, never).await;

    let left = script.lock().unwrap().len();
    (result, left)
}

/// The command goes on while its download is in the queue, and succeeds at
/// the snapshot in which the download has completed.
#[tokio::test]
async fn a_repair_ends_when_its_download_ends_and_succeeds_when_it_completed() {
    let dir = tempfile::tempdir().unwrap();

    let (result, left) = repair_and_watch(
        dir.path(),
        Ok(started()),
        vec![
            snapshot(None, vec![running(MINE)], vec![]),
            snapshot(Some(running(MINE)), vec![], vec![]),
            snapshot(None, vec![], vec![ended(MINE, COMPLETED)]),
            snapshot(None, vec![], vec![]),
        ],
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(left, 1, "it stopped at its download's ending");
}

/// The download failed after one of the two files had landed. The command
/// fails with how the download ended, the file still missing and the command
/// that fetches it.
#[tokio::test]
async fn a_download_that_fails_names_the_files_still_missing_and_what_fetches_them() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.gguf"), "weights").unwrap();
    let failed = ended(MINE, failed("Network error: connection reset"));

    let (result, _) = repair_and_watch(
        dir.path(),
        Ok(started()),
        vec![
            snapshot(Some(running(MINE)), vec![], vec![]),
            snapshot(None, vec![], vec![failed]),
        ],
    )
    .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        format!(
            "owner/mine:Q8_0: download failed: Network error: connection reset\n{}",
            missing("b.gguf")
        )
    );
}

/// A download someone cancelled, and one whose outcome is no longer in the
/// queue, are no more a repair than one that failed: each is an error that
/// names every file not back.
#[tokio::test]
async fn a_download_that_was_cancelled_or_lost_is_a_failed_repair() {
    let dir = tempfile::tempdir().unwrap();
    let cancelled = snapshot(None, vec![], vec![ended(MINE, DownloadOutcome::Cancelled)]);
    let lost = snapshot(None, vec![], vec![]);

    let (result, _) = repair_and_watch(dir.path(), Ok(started()), vec![cancelled]).await;
    assert_eq!(
        result.unwrap_err().to_string(),
        format!(
            "owner/mine:Q8_0: download cancelled\n{}",
            missing("a.gguf, b.gguf")
        )
    );

    let (result, _) = repair_and_watch(dir.path(), Ok(started()), vec![lost]).await;
    assert_eq!(
        result.unwrap_err().to_string(),
        format!(
            "owner/mine:Q8_0 left the download queue and how it ended is no longer recorded\n{}",
            missing("a.gguf, b.gguf")
        )
    );
}

/// Every file landed and the model could not be registered: the command
/// fails with that, and calls no file missing.
#[tokio::test]
async fn a_failed_download_whose_files_all_landed_calls_none_missing() {
    let dir = tempfile::tempdir().unwrap();
    for file in ["a.gguf", "b.gguf"] {
        std::fs::write(dir.path().join(file), "weights").unwrap();
    }
    let refused = ended(
        MINE,
        failed("Registration failed: Storage error: disk full"),
    );

    let snapshots = vec![snapshot(None, vec![], vec![refused])];
    let (result, _) = repair_and_watch(dir.path(), Ok(started()), snapshots).await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "owner/mine:Q8_0: download failed: Registration failed: Storage error: disk full"
    );
}

/// A repair the daemon refused is the command's error in the daemon's words,
/// and the queue is not read: there is no download to watch.
#[tokio::test]
async fn a_refused_repair_is_the_daemons_error_and_watches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let refused = Err(anyhow::anyhow!("daemon answered 500: nothing fetches it"));

    let unread = vec![snapshot(None, vec![], vec![ended(MINE, COMPLETED)])];
    let (result, left) = repair_and_watch(dir.path(), refused, unread).await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "daemon answered 500: nothing fetches it"
    );
    assert_eq!(left, 1, "nothing was read");
}

// ── The command, against a stand-in daemon ───────────────────────────────

/// Every request a stand-in daemon was sent but its health probe: the first
/// line, and the body.
type Asked = Arc<Mutex<Vec<(String, String)>>>;

/// A daemon on a loopback port that answers its health probe as a gglib
/// daemon does, a repair with [`started`], and each read of the download
/// queue with the next of `queue`.
fn stand_in_daemon(queue: Vec<QueueSnapshot>) -> (u16, Asked) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let asked = Asked::default();
    let seen = Arc::clone(&asked);
    let health = serde_json::json!({
        "service": "gglib-daemon",
        "fingerprint": gglib_build_info::FINGERPRINT,
    });
    std::thread::spawn(move || {
        let mut queue = VecDeque::from(queue);
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let (line, body) = read_request(&mut stream);
            let reply = if line.starts_with("GET /health ") {
                health.to_string()
            } else if line.starts_with("POST ") {
                seen.lock().unwrap().push((line, body));
                serde_json::to_string(&started()).unwrap()
            } else {
                seen.lock().unwrap().push((line, body));
                let next = queue.pop_front().expect("a read past the download's end");
                serde_json::to_string(&next).unwrap()
            };
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    (port, asked)
}

/// `gglib model repair` sends the model's repair to the daemon, reads the
/// daemon's queue until the download the daemon answered has ended, and
/// succeeds because that download completed.
#[tokio::test]
async fn the_command_sends_the_repair_to_the_daemon_and_ends_when_the_download_has() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    let (port, asked) = stand_in_daemon(vec![
        snapshot(Some(running(MINE)), vec![], vec![]),
        snapshot(None, vec![], vec![ended(MINE, COMPLETED)]),
    ]);
    let id = model.id.to_string();

    let argv = ["gglib", "model", "repair", id.as_str(), "--force"];
    let result = STAND_IN_PORT.scope(port, run(&ctx, &argv)).await;

    assert!(result.is_ok(), "{result:?}");
    let asked = asked.lock().unwrap().clone();
    let lines: Vec<&str> = asked.iter().map(|(line, _)| line.as_str()).collect();
    let queue = "GET /api/models/downloads/queue HTTP/1.1";
    let repair = format!("POST /api/models/{id}/repair HTTP/1.1");
    assert_eq!(lines, [repair.as_str(), queue, queue]);
    assert_eq!(asked[0].1, r#"{"shards":null}"#);
}

/// The download the repair queued failed with one of its two files back in
/// the folder the model's file is in. The command fails, names the shards it
/// asked for in the request, and calls missing the file that folder lacks:
/// it does not report a repair.
#[tokio::test]
async fn the_command_fails_when_the_download_does_and_names_what_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let (ctx, model) = library(dir.path()).await;
    std::fs::write(dir.path().join("a.gguf"), "weights").unwrap();
    let no_route = ended(MINE, failed("Network error: no route"));
    let (port, asked) = stand_in_daemon(vec![snapshot(None, vec![], vec![no_route])]);
    let id = model.id.to_string();

    let argv = ["gglib", "model", "repair", id.as_str(), "-f", "-s", "0,2"];
    let result = STAND_IN_PORT.scope(port, run(&ctx, &argv)).await;

    assert_eq!(
        result.unwrap_err().to_string(),
        format!(
            "owner/mine:Q8_0: download failed: Network error: no route\n{}",
            missing("b.gguf")
        )
    );
    assert_eq!(asked.lock().unwrap()[0].1, r#"{"shards":[0,2]}"#);
}
