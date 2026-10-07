//! Tests for which download the manager reports as running, and what the
//! queue does around it.

use gglib_core::download::DownloadPhase;
use gglib_core::ports::NoopEmitter;

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::test_support::{End, run_next};
use super::super::*;
use crate::test_hub::RepoHub;

/// A manager over a hub where every repository is one weights file and a
/// projector, so each download is two files.
fn manager() -> DownloadManagerImpl {
    DownloadManagerImpl::new(
        Arc::new(NoRegistrar),
        Arc::new(RepoHub::new(&[
            ("mmproj-F16.gguf", 300),
            ("zeta.Q8_0.gguf", 1_000),
        ])),
        Arc::new(NoopEmitter::new()),
        DownloadManagerConfig::default(),
    )
}

async fn queue(manager: &DownloadManagerImpl, repo: &str) -> DownloadId {
    let queued = manager
        .queue_download_smart(repo, Some("Q8_0".to_string()))
        .await
        .unwrap();
    queued.root_id
}

/// Run the next file off the queue to its end: on disk, or failed.
async fn finish_next(manager: &DownloadManagerImpl, on_disk: bool) {
    let end = if on_disk { End::OnDisk } else { End::Failed };
    run_next(manager, end).await;
}

/// Each row as its repository, phase and position.
async fn rows(manager: &DownloadManagerImpl) -> Vec<(String, DownloadPhase, u32)> {
    let snapshot = manager.get_queue_snapshot().await.unwrap();
    snapshot
        .rows()
        .map(|row| (row.model_id.clone(), row.phase, row.position))
        .collect()
}

fn row(repo: &str, phase: DownloadPhase, position: u32) -> (String, DownloadPhase, u32) {
    (repo.to_string(), phase, position)
}

#[tokio::test]
async fn a_download_that_has_not_started_is_waiting() {
    let manager = manager();
    queue(&manager, "owner/a").await;

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    assert!(snapshot.active.is_none());
    assert_eq!(snapshot.waiting.len(), 1);
    assert_eq!(snapshot.waiting[0].phase, DownloadPhase::Queued);
    assert_eq!(snapshot.waiting[0].position, 1);
}

/// The weights are in and the projector has not started: nothing is in
/// `active`, and the download is still the running one.
#[tokio::test]
async fn a_download_between_its_files_stays_active() {
    let manager = manager();
    queue(&manager, "owner/a").await;
    queue(&manager, "owner/b").await;
    finish_next(&manager, true).await;
    assert!(manager.active.lock().await.is_empty());

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    let running = snapshot.active.as_ref().expect("a running download");
    assert_eq!(running.id, "owner/a:Q8_0");
    assert_eq!(
        (running.phase, running.position),
        (DownloadPhase::Downloading, 1)
    );
    assert_eq!(running.text.file.as_deref(), Some("projector"), "its next");
    assert_eq!(snapshot.waiting.len(), 1);
    assert_eq!(snapshot.waiting[0].id, "owner/b:Q8_0");
    assert_eq!(snapshot.waiting[0].position, 2);
}

/// Between two files the row is read from the download's meter, which has
/// the bytes of the file that is in: the bar stays where that file left it.
/// The row is still downloading, and names the file that is next.
#[tokio::test]
async fn the_gap_row_is_active_at_the_meter_reading() {
    let manager = manager();
    queue(&manager, "owner/a").await;
    finish_next(&manager, true).await;

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    let running = snapshot.active.expect("a running download");
    assert_eq!(
        (running.downloaded_bytes, running.total_bytes),
        (1_000, Some(1_300)),
        "the weights' bytes stay on the bar"
    );
    assert_eq!(running.text.bytes, "1000 B / 1.27 KiB");
    assert_eq!(running.phase, DownloadPhase::Downloading);
    assert_eq!(running.text.file.as_deref(), Some("projector"));
}

#[tokio::test]
async fn the_monitor_does_not_exit_between_files() {
    let manager = manager();
    queue(&manager, "owner/a").await;
    finish_next(&manager, true).await;

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    assert!(snapshot.active.is_some());
    assert!(!snapshot.is_idle(), "idle is what the monitors exit on");
}

/// The weights failed and the projector then arrived, which leaves the
/// tracker holding a group that will never complete. With no file of it left
/// to run, it is not a download.
#[tokio::test]
async fn a_download_with_nothing_left_waiting_is_not_active() {
    let manager = manager();
    queue(&manager, "owner/a").await;
    finish_next(&manager, false).await;
    assert_eq!(
        rows(&manager).await,
        [row("owner/a", DownloadPhase::Queued, 1)],
        "after a failure the file left over is waiting, not running"
    );
    finish_next(&manager, true).await;
    assert!(manager.shard_tracker.lock().await.has_open_groups());

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    assert!(
        snapshot.is_idle(),
        "{:?}",
        snapshot.rows().collect::<Vec<_>>()
    );
}

/// Nothing is in `active`, and the head of the queue is the running
/// download's projector. A move to the front lands behind it.
#[tokio::test]
async fn reorder_in_the_gap_keeps_the_running_download_first() {
    let manager = manager();
    let _a = queue(&manager, "owner/a").await;
    let b = queue(&manager, "owner/b").await;
    let c = queue(&manager, "owner/c").await;
    finish_next(&manager, true).await;

    let position = manager.reorder_queue(&c, 2).await.unwrap();

    assert_eq!(position, 2);
    assert_eq!(
        rows(&manager).await,
        [
            row("owner/a", DownloadPhase::Downloading, 1),
            row("owner/c", DownloadPhase::Queued, 2),
            row("owner/b", DownloadPhase::Queued, 3),
        ]
    );

    // Position 1 is the running download's own.
    let position = manager.reorder_queue(&b, 1).await.unwrap();

    assert_eq!(position, 2);
    assert_eq!(
        rows(&manager).await,
        [
            row("owner/a", DownloadPhase::Downloading, 1),
            row("owner/b", DownloadPhase::Queued, 2),
            row("owner/c", DownloadPhase::Queued, 3),
        ]
    );
    let head = manager.queue.write().await.dequeue().unwrap();
    assert_eq!(head.id.model_id(), "owner/a", "its projector runs next");
}

/// A download queued while another is between its files is placed behind
/// it, at 2.
#[tokio::test]
async fn a_download_queued_in_the_gap_is_second() {
    let manager = manager();
    queue(&manager, "owner/a").await;
    finish_next(&manager, true).await;

    queue(&manager, "owner/b").await;

    assert_eq!(
        rows(&manager).await,
        [
            row("owner/a", DownloadPhase::Downloading, 1),
            row("owner/b", DownloadPhase::Queued, 2),
        ]
    );
}

/// The running download's own waiting file takes no place in the queue.
#[tokio::test]
async fn the_running_downloads_files_take_no_place_in_the_gap() {
    let manager = manager();
    manager.set_max_queue_size(1).await.unwrap();
    queue(&manager, "owner/a").await;
    finish_next(&manager, true).await;

    queue(&manager, "owner/b").await;
    let third = manager
        .queue_download_smart("owner/c", Some("Q8_0".to_string()))
        .await;

    assert!(matches!(third, Err(DownloadError::QueueFull { .. })));
}

// ── Starting a file ──────────────────────────────────────────────────────

/// `next_job` is stopped between taking the file off the queue and putting
/// it in `active`, by this test holding `active`. The queue must still be
/// held then, or a snapshot taken there would find the file in neither.
#[tokio::test]
async fn next_job_holds_the_queue_while_it_starts_a_file() {
    let manager = Arc::new(manager());
    queue(&manager, "owner/a").await;

    let active = manager.active.lock().await;
    let runner = Arc::clone(&manager);
    let started = tokio::spawn(async move { runner.next_job().await.is_some() });
    // Let `next_job` run until it waits on `active`: by then it holds the
    // queue, or it has let go of it with the file already taken.
    for _ in 0..100 {
        if manager
            .queue
            .try_read()
            .map_or(true, |q| q.pending_len() < 2)
        {
            break;
        }
        tokio::task::yield_now().await;
    }

    assert!(
        manager.queue.try_read().is_err(),
        "the queue was readable with a file in neither `pending` nor `active`"
    );
    drop(active);
    assert!(started.await.unwrap());
}

/// The reader's half: a snapshot stopped on `active` must hold the queue.
/// Were `active` read first, `next_job` could move a file between the two.
#[tokio::test]
async fn a_snapshot_holds_the_queue_while_it_reads_active() {
    let manager = Arc::new(manager());
    queue(&manager, "owner/a").await;

    let active = manager.active.lock().await;
    let reader = Arc::clone(&manager);
    let read = tokio::spawn(async move { reader.get_queue_snapshot().await.unwrap() });
    // Let the snapshot run until it waits on `active`.
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }

    assert!(manager.queue.try_write().is_err(), "the queue is not held");
    drop(active);
    assert_eq!(read.await.unwrap().waiting.len(), 1);
}

/// The snapshot `next_job` emits reads the queue, so the guard must be gone
/// by then: held, `next_job` would wait on itself for ever.
#[tokio::test]
async fn next_job_returns_after_publishing() {
    let manager = manager();
    queue(&manager, "owner/a").await;

    let started = tokio::time::timeout(Duration::from_secs(1), manager.next_job()).await;

    let (_, item, _, _) = started
        .expect("next_job returns within a second")
        .expect("a file was pending");
    assert_eq!(item.id.model_id(), "owner/a");
    assert_eq!(
        rows(&manager).await,
        [row("owner/a", DownloadPhase::Downloading, 1)]
    );
}
