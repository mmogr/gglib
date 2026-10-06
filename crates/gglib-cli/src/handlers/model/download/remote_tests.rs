//! The daemon monitor's loop: when it stops polling, and what it exits with.

use std::collections::VecDeque;
use std::sync::Mutex;

use super::super::monitor::tests::{
    COMPLETED, MINE, THEIRS, ended, failed, running, snapshot, waiting,
};
use super::*;

/// Watch `MINE` over a queue that reads as each of `snapshots` in turn.
/// Answers the result and how many were left unread. Reading past the last
/// is an error: the watch should have ended by then.
async fn watch(snapshots: Vec<QueueSnapshot>) -> (Result<()>, usize) {
    let script = Mutex::new(VecDeque::from(snapshots));
    let console = Arc::new(CliConsole::hidden());

    let result = watch_model(console, MINE, Duration::ZERO, || async {
        let next = script.lock().unwrap().pop_front();
        next.context("read past the last snapshot")
    })
    .await;

    let left = script.lock().unwrap().len();
    (result, left)
}

/// It watches its own repository: another download still running does not
/// keep it, and it stops reading once its own has completed.
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
        "download failed: owner/mine:Q8_0 \u{2014} Registration failed: Storage error: disk full"
    );
}

/// The download was taken off the queue while it waited. Nothing was
/// downloaded and nothing says so, and that is not a success.
#[tokio::test]
async fn a_download_gone_with_no_outcome_is_an_error() {
    let (result, _) = watch(vec![
        snapshot(Some(running(THEIRS)), vec![waiting(MINE)], vec![]),
        snapshot(Some(running(THEIRS)), vec![], vec![]),
    ])
    .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "owner/mine left the download queue and how it ended is not recorded"
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
