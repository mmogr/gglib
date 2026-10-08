//! The daemon monitor's loop: which download it watches, when it stops
//! polling, what it prints at the end, and what it exits with.

use std::collections::VecDeque;
use std::sync::Mutex;

use gglib_core::download::DownloadOutcome;

use super::super::monitor::tests::{
    COMPLETED, MINE, MINE_ID, THEIRS, ended, failed, mine, running, snapshot, waiting,
};
use super::*;

/// Watch the download of `MINE` over a queue that reads as each of `snapshots` in turn.
/// Answers the result and how many were left unread. Reading past the last
/// is an error: the watch should have ended by then.
async fn watch(snapshots: Vec<QueueSnapshot>) -> (Result<()>, usize) {
    let script = Mutex::new(VecDeque::from(snapshots));
    let console = Arc::new(CliConsole::hidden());

    let result = watch_download(console, &mine(), Duration::ZERO, || async {
        let next = script.lock().unwrap().pop_front();
        next.context("read past the last snapshot")
    })
    .await;

    let left = script.lock().unwrap().len();
    (result, left)
}

/// Queue with a request the daemon answers `queued`, then watch over a
/// queue that reads as each of `snapshots` in turn, never detaching.
/// Answers the result and how many snapshots were left unread.
async fn queue_and_watch(
    queued: Result<DownloadId>,
    snapshots: Vec<QueueSnapshot>,
) -> (Result<()>, usize) {
    let script = Mutex::new(VecDeque::from(snapshots));
    let console = Arc::new(CliConsole::hidden());
    let poll = || async {
        let next = script.lock().unwrap().pop_front();
        next.context("read past the last snapshot")
    };

    let never = std::future::pending();
    let result = queue_then_watch(console, async { queued }, Duration::ZERO, poll, never).await;

    let left = script.lock().unwrap().len();
    (result, left)
}

/// The request named a repository and no quantization, and the daemon
/// answered `owner/mine:Q8_0`. That is the download watched: the failure of
/// another quantization of the repository is not its outcome, and its own
/// completion is.
#[tokio::test]
async fn watches_the_download_the_daemon_answered() {
    let other_failed = ended("owner/mine:Q4_K_M", failed("no route"));

    let (result, left) = queue_and_watch(
        Ok(mine()),
        vec![
            snapshot(Some(running(MINE)), vec![], vec![other_failed.clone()]),
            snapshot(None, vec![], vec![other_failed, ended(MINE, COMPLETED)]),
            snapshot(None, vec![], vec![]),
        ],
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(left, 1, "it stopped at its own ending");
}

/// A queue request the daemon refused is the command's error, and the queue
/// is not read: there is no download to watch.
#[tokio::test]
async fn a_refused_queue_request_watches_nothing() {
    let refused = Err(anyhow::anyhow!("daemon answered 500: no such repository"));

    let (result, left) = queue_and_watch(refused, vec![snapshot(None, vec![], vec![])]).await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "daemon answered 500: no such repository"
    );
    assert_eq!(left, 1, "nothing was read");
}

/// Detaching ends the watch successfully while the download is still
/// running.
#[tokio::test]
async fn detaching_ends_the_watch_successfully() {
    let console = Arc::new(CliConsole::hidden());
    let poll = || async { Ok(snapshot(Some(running(MINE)), vec![], vec![])) };
    let every = Duration::from_hours(1);

    let watch = queue_then_watch(console, async { Ok(mine()) }, every, poll, async {});
    let result = tokio::time::timeout(Duration::from_secs(5), watch).await;

    assert!(matches!(result, Ok(Ok(()))), "{result:?}");
}

/// It watches its own download: another still running does not keep it, and
/// it stops reading once its own has completed.
#[tokio::test]
async fn exits_ok_when_its_download_completed() {
    let (result, left) = watch(vec![
        snapshot(Some(running(THEIRS)), vec![waiting(MINE)], vec![]),
        snapshot(Some(running(MINE)), vec![waiting(THEIRS)], vec![]),
        snapshot(Some(running(THEIRS)), vec![], vec![ended(MINE, COMPLETED)]),
        snapshot(Some(running(THEIRS)), vec![], vec![ended(MINE, COMPLETED)]),
    ])
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(left, 1, "it stopped at its own ending");
}

/// A download whose file the reader refused did complete: the model is in
/// the library. The command prints one line for it and no other, a tick and
/// the file and the reason as the daemon worded them, and succeeds.
#[tokio::test]
async fn a_download_added_without_its_details_prints_one_ticked_line_and_succeeds() {
    let note = "Downloaded model to models/zeta.Q8_0.gguf. zeta.Q8_0.gguf was added \
                without its details: Invalid GGUF format: Invalid magic number";
    let unread = DownloadOutcome::Completed {
        message: Some(note.to_string()),
    };
    let done = snapshot(None, vec![], vec![ended(MINE, unread)]);
    let (console, term) = CliConsole::on_unseen_term();

    let result = watch_download(Arc::new(console), &mine(), Duration::ZERO, || async {
        Ok(done.clone())
    })
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(term.written(), [format!("✓ {MINE_ID}: {note}")]);
}

/// Another download's failure is not this command's, and its own is: the
/// command fails with the reason.
#[tokio::test]
async fn exits_with_the_failure_of_its_own_download() {
    let theirs_failed = ended(THEIRS, failed("no route"));
    let refused = failed("Registration failed: Storage error: disk full");

    let (result, _) = watch(vec![
        snapshot(Some(running(MINE)), vec![], vec![theirs_failed.clone()]),
        snapshot(None, vec![], vec![theirs_failed, ended(MINE, refused)]),
    ])
    .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "owner/mine:Q8_0: download failed: Registration failed: Storage error: disk full"
    );
}

/// Its download ended before the first read, and another quantization of
/// the same repository has since failed. The command reads its own outcome
/// at the first look, and succeeds.
#[tokio::test]
async fn exits_on_the_first_read_with_its_own_outcome_alone() {
    let other_failed = ended("owner/mine:Q4_K_M", failed("no route"));

    let (result, left) = watch(vec![
        snapshot(None, vec![], vec![ended(MINE, COMPLETED), other_failed]),
        snapshot(None, vec![], vec![]),
    ])
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(left, 1, "one read was enough");
}

/// The download's outcome is gone from the queue: cleared, or pushed out by
/// later ones. Nothing says how it ended, and that is not a success.
#[tokio::test]
async fn a_download_gone_with_no_outcome_is_an_error() {
    let (result, _) = watch(vec![
        snapshot(Some(running(THEIRS)), vec![waiting(MINE)], vec![]),
        snapshot(Some(running(THEIRS)), vec![], vec![]),
    ])
    .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "owner/mine:Q8_0 left the download queue and how it ended is no longer recorded"
    );
}

/// A read of the queue that fails ends the watch with that error.
#[tokio::test]
async fn a_failed_read_ends_the_watch() {
    let (result, _) = watch(vec![]).await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "read past the last snapshot"
    );
}
