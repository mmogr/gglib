//! What a download fetched without the queue reads as, and how its files
//! are fetched.

use std::sync::Mutex;

use gglib_core::download::CompletionKey;

use super::*;
use crate::executor::FileProgress;
use crate::queue::DownloadQueue;

const fn at(bytes: u64, size: u64) -> FileProgress {
    FileProgress {
        bytes,
        wire: bytes,
        size: Some(size),
    }
}

fn id() -> DownloadId {
    DownloadId::new("owner/zeta-GGUF", Some("Q8_0"))
}

/// Weights of 1000 bytes and a projector of 300.
fn weights_and_projector() -> Vec<ResolvedFile> {
    vec![
        ResolvedFile::with_size("zeta.Q8_0.gguf", 1_000),
        ResolvedFile::projector("mmproj-F16.gguf", 300, None),
    ]
}

/// The queue's running row for a download of `files` that has `landed` of
/// them in and the next at `progress`: the files queued, taken off the queue
/// one at a time and metered as the manager does it.
fn queued_row(
    files: &[ResolvedFile],
    landed: &[FileProgress],
    progress: FileProgress,
) -> DownloadRow {
    let id = id();
    let key = CompletionKey::HfFile {
        repo_id: id.model_id().to_string(),
        revision: "unspecified".to_string(),
        filename_canon: "zeta.Q8_0.gguf".to_string(),
        quantization: Some("Q8_0".to_string()),
    };
    let mut queue = DownloadQueue::new(10);
    queue.queue_sharded(&id, &key, files, None).unwrap();

    let now = Instant::now();
    let mut item = queue.dequeue().unwrap();
    let mut meter = GroupMeter::for_group(item.shard_info.as_ref(), now);
    for last in landed {
        meter.observe(*last, None, now);
        meter.file_done();
        item = queue.dequeue().unwrap();
    }
    meter.observe(progress, None, now);

    let running = Running {
        id: item.id,
        phase: DownloadPhase::Downloading,
        file: item.shard_info,
    };
    let (row, _) = queue.download_rows(Some(&running), Some(&meter.reading()));
    row.expect("a running row")
}

/// An upgrade's row is the row the queue would show for the same download
/// at the same point, word for word, on its first file and on its last.
#[test]
fn upgrade_rows_come_from_the_shared_row_builder() {
    let files = weights_and_projector();
    let now = Instant::now();
    let solo = Arc::new(SoloDownload::new(id(), &files, now));

    (solo.progress())(at(400, 1_000));
    let row = solo.row(now);
    assert_eq!(row, queued_row(&files, &[], at(400, 1_000)));
    assert_eq!(row.text.title, "owner/zeta-GGUF:Q8_0");
    assert_eq!(row.text.file.as_deref(), Some("weights"));
    assert_eq!((row.downloaded_bytes, row.total_bytes), (400, Some(1_300)));
    assert_eq!(row.text.percent, "30.7%");

    (solo.progress())(at(1_000, 1_000));
    solo.file_done(now);
    (solo.progress())(at(150, 300));
    let row = solo.row(now);
    assert_eq!(row, queued_row(&files, &[at(1_000, 1_000)], at(150, 300)));
    assert_eq!(row.text.file.as_deref(), Some("projector"));
    assert_eq!(row.text.percent, "88.4%");
}

/// Each file's count starts from nothing, and the download's does not: it
/// holds what the earlier files came to while the next has nothing yet.
#[test]
fn upgrade_progress_never_rewinds_across_files() {
    let now = Instant::now();
    let solo = Arc::new(SoloDownload::new(id(), &weights_and_projector(), now));
    let mut seen = Vec::new();
    let mut read = || {
        let row = solo.row(now);
        seen.push((row.downloaded_bytes, row.text.percent));
    };

    (solo.progress())(at(400, 1_000));
    read();
    (solo.progress())(at(1_000, 1_000));
    read();
    solo.file_done(now);
    read();
    (solo.progress())(at(0, 300));
    read();
    (solo.progress())(at(150, 300));
    read();
    (solo.progress())(at(300, 300));
    solo.file_done(now);
    read();

    let seen: Vec<_> = seen
        .iter()
        .map(|(bytes, text)| (*bytes, text.as_str()))
        .collect();
    assert_eq!(
        seen,
        [
            (400, "30.7%"),
            (1_000, "76.9%"),
            (1_000, "76.9%"),
            (1_000, "76.9%"),
            (1_150, "88.4%"),
            (1_300, "100.0%"),
        ]
    );
}

/// A note is the row's status until bytes arrive again, and a note on one
/// file is not carried to the next.
#[test]
fn a_note_is_the_status_until_bytes_arrive_and_ends_with_its_file() {
    let now = Instant::now();
    let solo = Arc::new(SoloDownload::new(id(), &weights_and_projector(), now));
    (solo.progress())(at(400, 1_000));
    assert_eq!(solo.row(now).text.status, "Downloading");

    (solo.notices())("using direct transfer…");
    assert_eq!(solo.row(now).text.status, "using direct transfer…");
    (solo.progress())(at(500, 1_000));
    assert_eq!(solo.row(now).text.status, "Downloading");

    (solo.notices())("using direct transfer…");
    solo.file_done(now);
    assert_eq!(solo.row(now).text.status, "Downloading");
}

/// The rows a sink was handed, as bytes on disk and the file named.
type Seen = Arc<Mutex<Vec<(u64, Option<String>)>>>;

fn recording() -> (RowCallback, Seen) {
    let seen = Seen::default();
    let sink = Arc::clone(&seen);
    let rows: RowCallback = Arc::new(move |row: &DownloadRow| {
        sink.lock()
            .unwrap()
            .push((row.downloaded_bytes, row.text.file.clone()));
    });
    (rows, seen)
}

const NEVER: Duration = Duration::from_hours(1);

/// The files are fetched in order, and each is counted in before the next
/// starts: the row handed over as the weights land has their bytes, and the
/// one as the projector lands has both files'.
#[tokio::test]
async fn each_file_is_counted_in_as_it_lands() {
    let files = weights_and_projector();
    let (rows, seen) = recording();
    let fetched = Mutex::new(Vec::new());

    let result = fetch_solo(id(), &files, Some(&rows), NEVER, |index, progress, _| {
        fetched.lock().unwrap().push(index);
        let size = files[index].size.unwrap();
        async move {
            progress(at(size, size));
            Ok(())
        }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(*fetched.lock().unwrap(), [0, 1]);
    assert_eq!(
        *seen.lock().unwrap(),
        [
            (1_000, Some("projector".to_string())),
            (1_300, Some("projector".to_string())),
        ]
    );
}

/// While a file is fetched its row is handed over on every tick, moved or
/// not, and once more as the file lands.
#[tokio::test(start_paused = true)]
async fn the_row_is_handed_over_on_every_tick() {
    let files = [ResolvedFile::with_size("zeta.Q8_0.gguf", 1_000)];
    let (rows, seen) = recording();
    let tick = Duration::from_millis(250);

    let result = fetch_solo(
        id(),
        &files,
        Some(&rows),
        tick,
        |_, progress, _| async move {
            progress(at(100, 1_000));
            tokio::time::sleep(Duration::from_millis(600)).await;
            progress(at(1_000, 1_000));
            Ok(())
        },
    )
    .await;

    assert!(result.is_ok());
    // Ticks at 0, 250 and 500 ms, and then the file landing.
    assert_eq!(
        *seen.lock().unwrap(),
        [(100, None), (100, None), (100, None), (1_000, None)]
    );
}

/// The first file that fails ends the download with its error: the files
/// after it are not fetched, and it is not counted in.
#[tokio::test]
async fn a_file_that_fails_ends_the_download_there() {
    let files = weights_and_projector();
    let (rows, seen) = recording();
    let fetched = Mutex::new(Vec::new());

    let result = fetch_solo(id(), &files, Some(&rows), NEVER, |index, _, _| {
        fetched.lock().unwrap().push(index);
        async { Err(DownloadError::network("connection reset")) }
    })
    .await;

    assert_eq!(result, Err(DownloadError::network("connection reset")));
    assert_eq!(*fetched.lock().unwrap(), [0]);
    assert!(seen.lock().unwrap().is_empty());
}

/// With nobody to hand the row to, the files are fetched all the same.
#[tokio::test]
async fn files_are_fetched_with_no_sink() {
    let files = weights_and_projector();
    let fetched = Mutex::new(Vec::new());

    let result = fetch_solo(id(), &files, None, NEVER, |index, _, _| {
        fetched.lock().unwrap().push(index);
        async { Ok(()) }
    })
    .await;

    assert!(result.is_ok());
    assert_eq!(*fetched.lock().unwrap(), [0, 1]);
}
