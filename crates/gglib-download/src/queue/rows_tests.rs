//! Tests that the queue counts downloads: its rows.

use gglib_core::download::{CompletionKey, DownloadId, DownloadPhase};
use gglib_core::ports::ResolvedFile;

use super::super::{DownloadQueue, Reading, Running};

pub(super) fn id(model: &str) -> DownloadId {
    DownloadId::new(model, Some("Q8_0"))
}

pub(super) fn key(id: &DownloadId) -> CompletionKey {
    CompletionKey::HfFile {
        repo_id: id.model_id().to_string(),
        revision: "unspecified".to_string(),
        filename_canon: "test.gguf".to_string(),
        quantization: Some("Q8_0".to_string()),
    }
}

/// Queue `model` as one download of `files` files of 100 bytes each.
pub(super) fn add(
    queue: &mut DownloadQueue,
    model: &str,
    files: usize,
    running: Option<&DownloadId>,
) -> u32 {
    let id = id(model);
    let files: Vec<_> = (0..files)
        .map(|n| ResolvedFile::with_size(format!("{model}-{n}.gguf"), 100))
        .collect();
    queue
        .queue_sharded(&id, &key(&id), &files, running)
        .unwrap()
}

/// The pending files, each as its download and its number in the group.
pub(super) fn files(queue: &DownloadQueue) -> Vec<String> {
    queue
        .pending
        .iter()
        .map(|item| {
            let place = item.shard_info.as_ref().unwrap();
            format!("{}{}", item.id.model_id(), place.shard_index + 1)
        })
        .collect()
}

/// Take the head of `pending` as the runner does, and name its download as
/// the running one.
fn start(queue: &mut DownloadQueue) -> Running {
    let item = queue.dequeue().unwrap();
    Running {
        id: item.id,
        phase: DownloadPhase::Downloading,
        file: item.shard_info,
    }
}

/// Queue `model` as `shards` weights files of 100 bytes and a projector of
/// 50.
fn add_with_projector(queue: &mut DownloadQueue, model: &str, shards: usize) {
    let id = id(model);
    let mut files: Vec<_> = (0..shards)
        .map(|n| ResolvedFile::with_size(format!("{model}-{n}.gguf"), 100))
        .collect();
    files.push(ResolvedFile::projector("mmproj-F16.gguf", 50, None));
    queue.queue_sharded(&id, &key(&id), &files, None).unwrap();
}

// ── Rows ─────────────────────────────────────────────────────────────────

#[test]
fn a_waiting_file_of_the_running_download_is_not_a_row() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 2, None);
    add(&mut queue, "b", 1, None);
    let running = start(&mut queue);

    let snapshot = queue.snapshot(1, Some(&running), None);

    let rows: Vec<_> = snapshot
        .rows()
        .map(|row| (row.id.as_str(), row.phase, row.position))
        .collect();
    assert_eq!(
        rows,
        [
            ("a:Q8_0", DownloadPhase::Downloading, 1),
            ("b:Q8_0", DownloadPhase::Queued, 2),
        ]
    );
    assert_eq!(snapshot.active.map(|row| row.id).as_deref(), Some("a:Q8_0"));
    assert_eq!(snapshot.waiting.len(), 1);
}

#[test]
fn waiting_downloads_are_numbered_by_download() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "z", 1, None);
    add(&mut queue, "a", 2, None);
    add(&mut queue, "b", 1, None);
    let running = start(&mut queue);
    assert_eq!(files(&queue), ["a1", "a2", "b1"]);

    let snapshot = queue.snapshot(1, Some(&running), None);

    let positions: Vec<_> = snapshot
        .rows()
        .map(|row| (row.id.as_str(), row.position))
        .collect();
    assert_eq!(positions, [("z:Q8_0", 1), ("a:Q8_0", 2), ("b:Q8_0", 3)]);
    assert_eq!(snapshot.waiting.len(), 2);
}

/// A waiting row says how many shards its download has and how big it is,
/// and has moved nothing.
#[test]
fn a_waiting_row_carries_its_parts_and_its_size() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 3, None);

    let snapshot = queue.snapshot(1, None, None);

    assert!(snapshot.active.is_none());
    let [row] = &snapshot.waiting[..] else {
        panic!("one waiting row, not {:?}", snapshot.waiting);
    };
    assert_eq!((row.position, row.phase), (1, DownloadPhase::Queued));
    assert_eq!((row.downloaded_bytes, row.total_bytes), (0, Some(300)));
    assert_eq!(row.text.file.as_deref(), Some("3 parts"));
}

/// The running row carries what the meter reads, at position 1, and names
/// the file it is on. Without a reading it has its group's size and no
/// bytes.
#[test]
fn the_running_row_carries_the_reading() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 3, None);
    let running = start(&mut queue);
    let reading = Reading {
        bytes: 150,
        total: Some(300),
        speed_bps: Some(2_000_000.0),
        eta_seconds: Some(75.0),
        notice: None,
    };

    let (row, _) = queue.download_rows(Some(&running), Some(&reading));
    let row = row.unwrap();

    assert_eq!((row.downloaded_bytes, row.total_bytes), (150, Some(300)));
    assert_eq!(row.percent, Some(50.0));
    assert_eq!(row.speed_bps, Some(2_000_000.0));
    assert_eq!(row.text.file.as_deref(), Some("part 1/3"));
    assert_eq!(row.text.speed, "2.0 MB/s");

    let (unread, _) = queue.download_rows(Some(&running), None);
    let unread = unread.unwrap();
    assert_eq!(
        (unread.downloaded_bytes, unread.total_bytes),
        (0, Some(300))
    );
}

/// Three shards and a projector: the shards are parts of three, the
/// projector is the projector, and waiting it is a download of three parts.
#[test]
fn shards_read_part_i_of_n_and_never_count_the_projector() {
    let mut queue = DownloadQueue::new(10);
    add_with_projector(&mut queue, "a", 3);

    let (_, waiting) = queue.download_rows(None, None);
    assert_eq!(waiting[0].text.file.as_deref(), Some("3 parts"));
    assert_eq!(waiting[0].total_bytes, Some(350));

    let mut named = Vec::new();
    while !queue.pending.is_empty() {
        let running = start(&mut queue);
        let (row, _) = queue.download_rows(Some(&running), None);
        named.push(row.unwrap().text.file.unwrap());
    }
    assert_eq!(named, ["part 1/3", "part 2/3", "part 3/3", "projector"]);
}

/// One weights file and a projector: the row says which of the two it is on.
/// Waiting, it names no file: it is not a download in parts.
#[test]
fn one_shard_plus_projector_reads_weights_then_projector() {
    let mut queue = DownloadQueue::new(10);
    add_with_projector(&mut queue, "a", 1);

    let (_, waiting) = queue.download_rows(None, None);
    assert_eq!(waiting[0].text.file, None);

    let weights = start(&mut queue);
    let (row, _) = queue.download_rows(Some(&weights), None);
    assert_eq!(row.unwrap().text.file.as_deref(), Some("weights"));

    let projector = start(&mut queue);
    let (row, _) = queue.download_rows(Some(&projector), None);
    assert_eq!(row.unwrap().text.file.as_deref(), Some("projector"));
}

/// A download of one file names no file at all.
#[test]
fn a_download_of_one_file_names_no_file() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 1, None);
    let running = start(&mut queue);

    let (row, _) = queue.download_rows(Some(&running), None);

    assert_eq!(row.unwrap().text.file, None);
}

/// Between two files the next file of the running download is the head of
/// the queue, and of no other download.
#[test]
fn the_next_file_of_the_running_download_heads_the_queue() {
    let mut queue = DownloadQueue::new(10);
    add(&mut queue, "a", 3, None);
    queue.dequeue().unwrap();

    let next = queue.next_file_of(&id("a")).unwrap();

    assert_eq!(next.shard_index, 1);
    assert!(queue.next_file_of(&id("b")).is_none());
}

/// The queue is full when as many downloads wait as may, and the running
/// download is not one of them.
#[test]
fn full_counts_the_waiting_downloads() {
    let mut queue = DownloadQueue::new(1);
    add(&mut queue, "a", 2, None);
    let running = start(&mut queue);
    assert!(!queue.snapshot(1, Some(&running), None).full);

    add(&mut queue, "b", 3, Some(&running.id));

    let snapshot = queue.snapshot(2, Some(&running), None);
    assert!(snapshot.full);
    assert_eq!((snapshot.revision, snapshot.max_size), (2, 1));
}
