//! A download the user stops ends as a whole, whatever it was doing.

use gglib_core::download::QueueRunSummary;

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::test_support::{End, end_started, run_next, start_next};
use super::super::*;
use super::tests::{Fixture, ID, REPO, queued};

pub(super) fn id() -> DownloadId {
    DownloadId::new(REPO, Some("Q8_0"))
}

/// Queue a second download, of two files like the first, and answer its id.
async fn queue_other(f: &Fixture, repo: &str) -> DownloadId {
    f.manager
        .queue_download_smart(repo, Some("Q8_0".to_string()))
        .await
        .unwrap()
}

/// The finished list of the last snapshot sent, as ids and outcomes.
pub(super) fn finished(f: &Fixture) -> Vec<(String, DownloadOutcome)> {
    let last = f.recorded.snapshots().pop().expect("a snapshot was sent");
    last.finished
        .into_iter()
        .map(|ended| (ended.id, ended.outcome))
        .collect()
}

/// Every run summary sent.
pub(super) fn summaries(f: &Fixture) -> Vec<QueueRunSummary> {
    f.recorded
        .events()
        .into_iter()
        .filter_map(|event| match event {
            DownloadEvent::QueueRunComplete { summary } => Some(summary),
            _ => None,
        })
        .collect()
}

fn cancelled(id: &str) -> (String, DownloadOutcome) {
    (id.to_string(), DownloadOutcome::Cancelled)
}

/// A download cancelled while its weights are fetched does not go on to its
/// projector: the worker is told to stop, and when it has, every file of
/// the download is off the queue and the download is cancelled once.
#[tokio::test]
async fn cancelling_removes_its_waiting_files() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let weights = start_next(&f.manager).await;

    f.manager.cancel_download(&id()).await.unwrap();
    assert!(weights.cancel.is_cancelled(), "the worker is told to stop");
    assert!(
        f.recorded.endings().is_empty(),
        "it has not ended until its worker has"
    );
    end_started(&f.manager, weights, End::Cancelled).await;

    assert_eq!(f.manager.queue.read().await.pending_len(), 0);
    assert!(f.manager.next_job().await.is_none(), "nothing left to run");
    let last = f.recorded.snapshots().pop().unwrap();
    assert!(last.is_idle(), "{:?}", last.rows().collect::<Vec<_>>());
    assert_eq!(finished(&f), [cancelled(ID)]);
    assert_eq!(f.recorded.endings().len(), 1);
}

/// Between its two files a download has nothing active. Cancelled then, it
/// ends there: its projector leaves the queue, its group is closed, and the
/// run is summed up with one cancelled download.
#[tokio::test]
async fn cancel_in_the_gap_ends_the_download_and_drains() {
    let f = queued(Arc::new(NoRegistrar)).await;
    run_next(&f.manager, End::OnDisk).await;
    assert!(f.manager.shard_tracker.lock().await.has_open_groups());

    f.manager.cancel_download(&id()).await.unwrap();

    assert_eq!(f.manager.queue.read().await.pending_len(), 0);
    assert!(!f.manager.shard_tracker.lock().await.has_open_groups());
    assert!(f.manager.meters().is_empty());
    assert_eq!(finished(&f), [cancelled(ID)]);
    let endings = f.recorded.endings();
    assert!(
        matches!(&endings[..], [DownloadEvent::DownloadCancelled { id, .. }] if id == ID),
        "{endings:?}"
    );
    let [summary] = &summaries(&f)[..] else {
        panic!("the queue drained once, not {:?}", summaries(&f).len());
    };
    assert_eq!(
        (
            summary.unique_models_cancelled,
            summary.unique_models_downloaded
        ),
        (1, 0)
    );
}

/// A download that never started is ended the same way, by either call:
/// it leaves a cancelled outcome, so nothing watching it sees it vanish.
#[tokio::test]
async fn a_download_stopped_before_it_starts_is_recorded_as_cancelled() {
    let f = queued(Arc::new(NoRegistrar)).await;
    f.manager.cancel_download(&id()).await.unwrap();
    assert_eq!(finished(&f), [cancelled(ID)]);
    assert_eq!(f.recorded.endings().len(), 1);
    assert_eq!(summaries(&f).len(), 1);

    let f = queued(Arc::new(NoRegistrar)).await;
    f.manager.remove_from_queue(&id()).await.unwrap();
    assert_eq!(finished(&f), [cancelled(ID)]);
    assert_eq!(f.recorded.endings().len(), 1);
}

/// Removing the download being fetched cancels it: its worker is told to
/// stop, and it ends cancelled even when the file landed all the same.
#[tokio::test]
async fn delete_on_the_active_id_cancels_it() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let weights = start_next(&f.manager).await;

    f.manager.remove_from_queue(&id()).await.unwrap();

    assert!(weights.cancel.is_cancelled());
    let running = f.manager.get_queue_snapshot().await.unwrap();
    assert!(
        running.active.is_some(),
        "still running until its worker stops"
    );
    assert!(running.finished.is_empty());
    end_started(&f.manager, weights, End::OnDisk).await;
    assert_eq!(finished(&f), [cancelled(ID)]);
    assert_eq!(f.manager.queue.read().await.pending_len(), 0);
}

/// Removing a download that has ended drops its entry; cancelling one does
/// not, and says it is not in the queue. A download that completed as the
/// cancel was sent keeps its outcome for whoever is waiting to read it.
#[tokio::test]
async fn removing_drops_a_finished_entry_and_cancelling_does_not() {
    let f = queued(Arc::new(NoRegistrar)).await;
    run_next(&f.manager, End::Failed).await;
    assert_eq!(finished(&f).len(), 1);

    let cancel = f.manager.cancel_download(&id()).await;
    assert!(matches!(cancel, Err(DownloadError::NotInQueue { .. })));
    let served = f.manager.get_queue_snapshot().await.unwrap();
    assert_eq!(served.finished.len(), 1);

    f.manager.remove_from_queue(&id()).await.unwrap();
    assert!(finished(&f).is_empty());
    let again = f.manager.remove_from_queue(&id()).await;
    assert!(matches!(again, Err(DownloadError::NotInQueue { .. })));
}

/// Cancelling everything ends each download with its own outcome: the one
/// between two files and the ones that never started.
#[tokio::test]
async fn cancel_all_records_each_download() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let other = queue_other(&f, "owner/other").await.to_string();
    let third = queue_other(&f, "owner/third").await.to_string();
    run_next(&f.manager, End::OnDisk).await;

    f.manager.cancel_all().await.unwrap();

    assert_eq!(
        finished(&f),
        [cancelled(ID), cancelled(&other), cancelled(&third)]
    );
    assert_eq!(f.recorded.endings().len(), 3);
    assert_eq!(f.manager.queue.read().await.pending_len(), 0);
    assert!(!f.manager.shard_tracker.lock().await.has_open_groups());
    let [summary] = &summaries(&f)[..] else {
        panic!("the queue drained once, not {:?}", summaries(&f).len());
    };
    assert_eq!(summary.unique_models_cancelled, 3);
}

/// With a file being fetched, cancelling everything tells its worker to
/// stop and ends the waiting downloads at once. The fetched one ends when
/// its worker has, and takes its projector with it.
#[tokio::test]
async fn stopping_everything_leaves_the_fetched_download_to_its_worker() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let other = queue_other(&f, "owner/other").await.to_string();
    let weights = start_next(&f.manager).await;

    f.manager.stop_all().await;

    assert!(weights.cancel.is_cancelled());
    assert_eq!(finished(&f), [cancelled(&other)]);
    assert_eq!(
        f.manager.queue.read().await.pending_len(),
        1,
        "its projector"
    );
    end_started(&f.manager, weights, End::Cancelled).await;
    assert_eq!(finished(&f), [cancelled(&other), cancelled(ID)]);
    assert_eq!(f.manager.queue.read().await.pending_len(), 0);
    assert_eq!(summaries(&f).len(), 1);
}

/// A download that ended and is queued again is waiting, and its old
/// outcome is gone from the snapshot: the new run is not read as its last.
#[tokio::test]
async fn requeueing_clears_the_old_outcome() {
    let f = queued(Arc::new(NoRegistrar)).await;
    queue_other(&f, "owner/other").await;
    run_next(&f.manager, End::Failed).await;
    run_next(&f.manager, End::Failed).await;
    assert_eq!(finished(&f).len(), 2);

    f.manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();

    let last = f.recorded.snapshots().pop().unwrap();
    let waiting: Vec<&str> = last.waiting.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(waiting, [ID]);
    let ended: Vec<&str> = last.finished.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ended, ["owner/other:Q8_0"]);
}
