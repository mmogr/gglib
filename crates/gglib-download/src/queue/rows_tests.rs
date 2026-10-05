//! Tests that the queue counts downloads: its rows, positions, capacity and
//! reordering.

use gglib_core::download::{CompletionKey, DownloadError, DownloadId, DownloadStatus};
use gglib_core::ports::ResolvedFile;

use super::super::DownloadQueue;

fn id(model: &str) -> DownloadId {
    DownloadId::new(model, Some("Q8_0"))
}

fn key(id: &DownloadId) -> CompletionKey {
    CompletionKey::HfFile {
        repo_id: id.model_id().to_string(),
        revision: "unspecified".to_string(),
        filename_canon: "test.gguf".to_string(),
        quantization: Some("Q8_0".to_string()),
    }
}

/// Queue `model` as one download of `files` files of 100 bytes each.
fn add(queue: &mut DownloadQueue, model: &str, files: usize, running: Option<&DownloadId>) -> u32 {
    let id = id(model);
    let files: Vec<_> = (0..files)
        .map(|n| ResolvedFile::with_size(format!("{model}-{n}.gguf"), 100))
        .collect();
    queue
        .queue_sharded(&id, &key(&id), &files, running)
        .unwrap()
}

/// The pending files, each as its download and its number in the group.
fn files(queue: &DownloadQueue) -> Vec<String> {
    queue
        .pending
        .iter()
        .map(|item| {
            let place = item.shard_info.as_ref().unwrap();
            format!("{}{}", item.id.model_id(), place.shard_index + 1)
        })
        .collect()
}

/// Take the head of `pending` as the runner does, and answer its active row.
fn start(queue: &mut DownloadQueue) -> gglib_core::download::QueuedDownload {
    queue
        .dequeue()
        .unwrap()
        .to_dto(1, DownloadStatus::Downloading)
}

// ── Rows ─────────────────────────────────────────────────────────────────

#[test]
fn a_waiting_file_of_the_running_download_is_not_a_row() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 2, None);
    add(&mut queue, "b", 1, None);
    let running = start(&mut queue);

    let snapshot = queue.snapshot(Some(running));

    let rows: Vec<_> = snapshot
        .items
        .iter()
        .map(|row| (row.id.as_str(), row.status, row.position))
        .collect();
    assert_eq!(
        rows,
        [
            ("a:Q8_0", DownloadStatus::Downloading, 1),
            ("b:Q8_0", DownloadStatus::Queued, 2),
        ]
    );
    assert_eq!((snapshot.active_count, snapshot.pending_count), (1, 1));
}

#[test]
fn waiting_downloads_are_numbered_by_download() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "z", 1, None);
    add(&mut queue, "a", 2, None);
    add(&mut queue, "b", 1, None);
    let running = start(&mut queue);
    assert_eq!(files(&queue), ["a1", "a2", "b1"]);

    let snapshot = queue.snapshot(Some(running));

    let positions: Vec<_> = snapshot
        .items
        .iter()
        .map(|row| (row.id.as_str(), row.position))
        .collect();
    assert_eq!(positions, [("z:Q8_0", 1), ("a:Q8_0", 2), ("b:Q8_0", 3)]);
    assert_eq!(snapshot.pending_count, 2);
}

/// A waiting row is the download's first file, so it says how many shards
/// the download has.
#[test]
fn a_waiting_row_carries_the_place_of_its_first_file() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 3, None);

    let snapshot = queue.snapshot(None);

    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].position, 1);
    assert_eq!((snapshot.active_count, snapshot.pending_count), (0, 1));
    let place = snapshot.items[0].shard_info.as_ref().unwrap();
    assert_eq!((place.shard_index, place.total_shards), (0, 3));
}

/// Between two files the running download is a downloading row at position
/// 1, holding the bytes of the files already in.
#[test]
fn the_row_between_files_keeps_the_bytes_already_in() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 3, None);
    queue.dequeue().unwrap();

    let row = queue.between_files_row(&id("a")).unwrap();

    assert_eq!(row.id, "a:Q8_0");
    assert_eq!((row.status, row.position), (DownloadStatus::Downloading, 1));
    assert_eq!((row.downloaded_bytes, row.total_bytes), (100, 300));
    assert_eq!(row.shard_info.map(|place| place.shard_index), Some(1));
    assert!(queue.between_files_row(&id("b")).is_none());
}

// ── Positions and capacity ───────────────────────────────────────────────

#[test]
fn a_new_download_is_placed_behind_the_downloads_not_the_files() {
    let mut queue = DownloadQueue::new(10);
    let running = id("z");

    assert_eq!(add(&mut queue, "a", 3, Some(&running)), 2);
    assert_eq!(add(&mut queue, "b", 1, Some(&running)), 3);

    let mut idle = DownloadQueue::new(10);
    assert_eq!(add(&mut idle, "a", 3, None), 1);
    assert_eq!(add(&mut idle, "b", 1, None), 2);
}

/// Ten shards and a projector are one download: they fit an empty queue of
/// ten, and so do nine more downloads. The eleventh is refused.
#[test]
fn capacity_counts_waiting_downloads() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "big", 11, None);
    for n in 1..10 {
        add(&mut queue, &format!("m{n}"), 1, None);
    }

    let eleventh = id("one-more");
    let refused = queue.queue_sharded(
        &eleventh,
        &key(&eleventh),
        &[ResolvedFile::new("f.gguf")],
        None,
    );

    assert!(matches!(
        refused,
        Err(DownloadError::QueueFull { max_size: 10 })
    ));
    assert_eq!(queue.pending_len(), 20);
}

/// The running download's own pending files take no place in the queue.
#[test]
fn the_running_downloads_files_take_no_place() {
    let mut queue = DownloadQueue::new(1);
    add(&mut queue, "a", 3, None);
    queue.dequeue().unwrap();
    let running = id("a");

    add(&mut queue, "b", 1, Some(&running));

    let full = id("c");
    let refused = queue.queue_sharded(
        &full,
        &key(&full),
        &[ResolvedFile::new("f.gguf")],
        Some(&running),
    );
    assert!(matches!(refused, Err(DownloadError::QueueFull { .. })));
}

// ── Reordering ───────────────────────────────────────────────────────────

#[test]
fn reorder_by_download_position() {
    let mut queue = DownloadQueue::new(10);
    let running = id("z");
    add(&mut queue, "a", 2, Some(&running));
    add(&mut queue, "b", 1, Some(&running));
    add(&mut queue, "c", 1, Some(&running));

    let position = queue.reorder(&id("c"), 3, Some(&running)).unwrap();

    assert_eq!(position, 3);
    assert_eq!(files(&queue), ["a1", "a2", "c1", "b1"]);
}

#[test]
fn reorder_never_splits_the_running_download() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 2, None);
    add(&mut queue, "b", 1, None);
    add(&mut queue, "c", 1, None);
    queue.dequeue().unwrap();
    let running = id("a");
    assert_eq!(files(&queue), ["a2", "b1", "c1"]);

    let position = queue.reorder(&id("c"), 2, Some(&running)).unwrap();

    assert_eq!(position, 2);
    assert_eq!(files(&queue), ["a2", "c1", "b1"]);

    // Position 1 is the running download's: a request for it lands on the
    // first waiting place.
    let position = queue.reorder(&id("b"), 1, Some(&running)).unwrap();

    assert_eq!(position, 2);
    assert_eq!(files(&queue), ["a2", "b1", "c1"]);
}

#[test]
fn the_running_download_is_not_moved() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 3, None);
    add(&mut queue, "b", 1, None);
    queue.dequeue().unwrap();
    let running = id("a");

    let position = queue.reorder(&running, 3, Some(&running)).unwrap();

    assert_eq!(position, 1);
    assert_eq!(files(&queue), ["a2", "a3", "b1"]);
}

/// A position past the end is the last place, and that is the position
/// answered.
#[test]
fn a_position_past_the_end_is_the_last_place() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 2, None);
    add(&mut queue, "b", 1, None);
    add(&mut queue, "c", 2, None);

    let position = queue.reorder(&id("a"), 9, None).unwrap();

    assert_eq!(position, 3);
    assert_eq!(files(&queue), ["b1", "c1", "c2", "a1", "a2"]);
}
