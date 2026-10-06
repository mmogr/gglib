//! Tests that the queue counts downloads: its positions, capacity and
//! reordering.

use gglib_core::download::DownloadError;
use gglib_core::ports::ResolvedFile;

use super::super::DownloadQueue;
use super::tests::{add, files, id, key};

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
