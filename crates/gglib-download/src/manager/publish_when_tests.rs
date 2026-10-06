//! When a snapshot is published: at every change to what one carries.

use std::time::Instant;

use gglib_core::download::Quantization;
use gglib_core::ports::DownloadRequest;

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::group_registration_tests::RecordingRegistrar;
use super::super::test_support::{End, reading, run_next};
use super::super::*;
use super::tests::{Fixture, ID, REPO, queued};

fn id() -> DownloadId {
    DownloadId::new(REPO, Some("Q8_0"))
}

/// How many snapshots have been sent, and the last of them.
fn sent(f: &Fixture) -> (usize, QueueSnapshot) {
    let snapshots = f.recorded.snapshots();
    (snapshots.len(), snapshots.last().cloned().unwrap())
}

/// A file starting moves its download from waiting to running, and that is
/// sent without waiting for the meter's first tick.
#[tokio::test]
async fn starting_a_file_publishes_the_running_row() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let (before, queued) = sent(&f);
    assert!(queued.active.is_none());

    f.manager.next_job().await.unwrap();

    let (after, started) = sent(&f);
    assert_eq!(after, before + 1);
    let row = started.active.expect("a running row");
    assert_eq!(
        (row.id.as_str(), row.phase),
        (ID, DownloadPhase::Downloading)
    );
    assert!(started.waiting.is_empty());
}

/// Both ways of queueing publish the new row.
#[tokio::test]
async fn queueing_publishes_the_new_row() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let (before, _) = sent(&f);

    f.manager
        .queue_download(DownloadRequest::new("owner/other", Quantization::Q8_0))
        .await
        .unwrap();

    let (after, snapshot) = sent(&f);
    assert_eq!(after, before + 1);
    let waiting: Vec<&str> = snapshot.waiting.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(waiting, [ID, "owner/other:Q8_0"]);
}

/// A waiting download taken off the queue is gone from the next snapshot
/// sent, whichever call took it off.
#[tokio::test]
async fn taking_a_waiting_download_off_publishes_the_shorter_queue() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let (before, _) = sent(&f);

    f.manager.cancel_download(&id()).await.unwrap();

    let (after, snapshot) = sent(&f);
    assert_eq!(after, before + 1);
    assert!(snapshot.is_idle());

    let f = queued(Arc::new(NoRegistrar)).await;
    let (before, _) = sent(&f);

    f.manager.remove_from_queue(&id()).await.unwrap();

    let (after, snapshot) = sent(&f);
    assert_eq!(after, before + 1);
    assert!(snapshot.is_idle());
}

/// The snapshot carries the queue's size and whether it is full, so a new
/// size is a new snapshot.
#[tokio::test]
async fn a_new_queue_size_is_published() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let (before, snapshot) = sent(&f);
    assert!(!snapshot.full);

    f.manager.set_max_queue_size(1).await.unwrap();

    let (after, snapshot) = sent(&f);
    assert_eq!(after, before + 1);
    assert_eq!((snapshot.max_size, snapshot.full), (1, true));
}

/// A run of one download is summed up once, after the download has ended:
/// a download running with nothing behind it is not a drained queue.
#[tokio::test]
async fn a_run_is_summed_up_once_when_its_download_has_ended() {
    let f = queued(Arc::new(RecordingRegistrar::default())).await;

    run_next(&f.manager, End::OnDisk).await;
    let midway = f.recorded.events();
    assert!(
        !midway
            .iter()
            .any(|event| matches!(event, DownloadEvent::QueueRunComplete { .. })),
        "the projector is still to come"
    );
    run_next(&f.manager, End::OnDisk).await;

    let events = f.recorded.events();
    let summaries: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event, DownloadEvent::QueueRunComplete { .. }))
        .map(|(at, _)| at)
        .collect();
    let completed = events
        .iter()
        .position(|event| matches!(event, DownloadEvent::DownloadCompleted { .. }))
        .expect("the download completed");
    let [summary] = summaries[..] else {
        panic!("one summary, not {}", summaries.len());
    };
    assert!(summary > completed, "the summary follows the ending");
}

/// A download of two files has a time remaining: its meter is started with
/// the size of both, which no one file's reading carries.
#[tokio::test]
async fn a_two_file_download_has_a_time_remaining() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let (_lease, item, _cancel, _progress) = f.manager.next_job().await.unwrap();
    let start = Instant::now();

    for tick in 0..=40_u32 {
        let at = start + PROGRESS_TICK * tick;
        f.manager
            .observe(&item.id, &reading(u64::from(tick) * 10, 1_000), at);
    }

    let row = f
        .manager
        .get_queue_snapshot()
        .await
        .unwrap()
        .active
        .unwrap();
    assert_eq!((row.downloaded_bytes, row.total_bytes), (400, Some(1_300)));
    // 900 bytes are left at 40 bytes a second, which the estimator is
    // still settling on.
    let eta = row.eta_seconds.expect("a time remaining");
    assert!((15.0..35.0).contains(&eta), "{eta}");
}
