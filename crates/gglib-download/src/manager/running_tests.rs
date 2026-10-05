//! Tests for which download the manager reports as running, and what the
//! queue does around it.

use std::path::Path;

use gglib_core::download::{DownloadStatus, Quantization};
use gglib_core::ports::NoopEmitter;

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::worker::CompletedJob;
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

/// Take the next file off the queue and finish it as the worker reports it:
/// on disk, or failed.
async fn finish_next(manager: &DownloadManagerImpl, on_disk: bool) {
    let item = manager.queue.write().await.dequeue().unwrap();
    let name = item.shard_info.as_ref().unwrap().filename.clone();
    let path = Path::new("models").join(&name);
    let result = if on_disk {
        Ok(CompletedJob {
            primary_path: path.clone(),
            all_paths: vec![path],
            repo_id: item.id.model_id().to_string(),
            commit_sha: "abc123".to_string(),
            quantization: Quantization::Q8_0,
            files: vec![name],
        })
    } else {
        Err(DownloadError::network("connection reset"))
    };
    manager.handle_job_result(&item, result).await;
}

/// Each row as its repository, status and position.
async fn rows(manager: &DownloadManagerImpl) -> Vec<(String, DownloadStatus, u32)> {
    let snapshot = manager.get_queue_snapshot().await.unwrap();
    snapshot
        .items
        .into_iter()
        .map(|row| (row.model_id, row.status, row.position))
        .collect()
}

fn row(repo: &str, status: DownloadStatus, position: u32) -> (String, DownloadStatus, u32) {
    (repo.to_string(), status, position)
}

/// What both of the in-process monitors exit on: nothing active and nothing
/// pending (`is_queue_finished` in the CLI's `interactive.rs`).
const fn the_monitor_would_exit(snapshot: &QueueSnapshot) -> bool {
    snapshot.active_count == 0 && snapshot.pending_count == 0
}

#[tokio::test]
async fn a_download_that_has_not_started_is_waiting() {
    let manager = manager();
    queue(&manager, "owner/a").await;

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    assert_eq!((snapshot.active_count, snapshot.pending_count), (0, 1));
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].status, DownloadStatus::Queued);
    assert_eq!(snapshot.items[0].position, 1);
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

    assert_eq!((snapshot.active_count, snapshot.pending_count), (1, 1));
    assert_eq!(snapshot.items.len(), 2);
    let running = &snapshot.items[0];
    assert_eq!(running.id, "owner/a:Q8_0");
    assert_eq!(
        (running.status, running.position),
        (DownloadStatus::Downloading, 1)
    );
    assert_eq!(
        (running.downloaded_bytes, running.total_bytes),
        (1_000, 1_300),
        "the weights' bytes stay on the bar"
    );
    let next = running.shard_info.as_ref().unwrap();
    assert_eq!(next.filename, "mmproj-F16.gguf");
    assert_eq!(snapshot.items[1].id, "owner/b:Q8_0");
    assert_eq!(snapshot.items[1].position, 2);
}

#[tokio::test]
async fn the_monitor_does_not_exit_between_files() {
    let manager = manager();
    queue(&manager, "owner/a").await;
    finish_next(&manager, true).await;

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    assert_eq!(snapshot.active_count, 1);
    assert!(!the_monitor_would_exit(&snapshot));
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
        [row("owner/a", DownloadStatus::Queued, 1)],
        "after a failure the file left over is waiting, not running"
    );
    finish_next(&manager, true).await;
    assert!(manager.shard_tracker.lock().await.has_open_groups());

    let snapshot = manager.get_queue_snapshot().await.unwrap();

    assert!(snapshot.items.is_empty(), "{:?}", snapshot.items);
    assert_eq!((snapshot.active_count, snapshot.pending_count), (0, 0));
    assert!(the_monitor_would_exit(&snapshot));
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
            row("owner/a", DownloadStatus::Downloading, 1),
            row("owner/c", DownloadStatus::Queued, 2),
            row("owner/b", DownloadStatus::Queued, 3),
        ]
    );

    // Position 1 is the running download's own.
    let position = manager.reorder_queue(&b, 1).await.unwrap();

    assert_eq!(position, 2);
    assert_eq!(
        rows(&manager).await,
        [
            row("owner/a", DownloadStatus::Downloading, 1),
            row("owner/b", DownloadStatus::Queued, 2),
            row("owner/c", DownloadStatus::Queued, 3),
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
            row("owner/a", DownloadStatus::Downloading, 1),
            row("owner/b", DownloadStatus::Queued, 2),
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
