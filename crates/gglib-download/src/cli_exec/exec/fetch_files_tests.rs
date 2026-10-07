//! `fetch_files` with a hand-written fetch in place of the transfer: what
//! each file's plan is given, and what of it reaches the download's row.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use gglib_core::download::DownloadRow;

use super::*;
use crate::executor::FileProgress;

const NOTE: &str = "using direct transfer…";

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

fn transfer(destination: &Path) -> Transfer<'_> {
    Transfer {
        repo_id: "owner/zeta-GGUF",
        revision: "2222222",
        destination,
        token: Some("a-token"),
        force: true,
    }
}

const fn at(bytes: u64, size: u64) -> FileProgress {
    FileProgress {
        bytes,
        wire: bytes,
        size: Some(size),
    }
}

/// The rows a sink was handed, as bytes on disk and the status.
type Seen = Arc<Mutex<Vec<(u64, String)>>>;

fn recording() -> (RowCallback, Seen) {
    let seen = Seen::default();
    let sink = Arc::clone(&seen);
    let rows: RowCallback = Arc::new(move |row: &DownloadRow| {
        sink.lock()
            .unwrap()
            .push((row.downloaded_bytes, row.text.status.clone()));
    });
    (rows, seen)
}

/// What a transfer reports through its plan is on the row the caller is
/// handed: the bytes through the plan's progress sink, a note through its
/// notice sink, on the manager's tick while the file is fetched and once
/// more as it lands.
#[tokio::test(start_paused = true)]
async fn what_a_file_reports_through_its_plan_is_on_the_row() {
    let files = weights_and_projector();
    let destination = PathBuf::from("/models/owner/zeta-GGUF");
    let transfer = transfer(&destination);
    let (rows, seen) = recording();

    // Half of the file and a note, and the rest of it 600 ms later.
    let fetch_one = |plan: DownloadPlan<'_>| async move {
        let progress = plan.progress.expect("a sink for the file's progress");
        let notice = plan.notice.expect("a sink for the file's notes");
        let size = plan.expected_size.expect("the file's size");
        progress(at(size / 2, size));
        notice(NOTE);
        tokio::time::sleep(Duration::from_millis(600)).await;
        progress(at(size, size));
        Ok(())
    };
    let result = fetch_files(id(), &files, &transfer, Some(&rows), fetch_one).await;

    assert_eq!(result, Ok(()));
    let seen: Vec<_> = seen.lock().unwrap().clone();
    let seen: Vec<_> = seen.iter().map(|(b, s)| (*b, s.as_str())).collect();
    assert_eq!(
        seen,
        [
            // The weights: ticks at 0, 250 and 500 ms, and landing at 600.
            (500, NOTE),
            (500, NOTE),
            (500, NOTE),
            (1_000, "Downloading"),
            // The projector: ticks at 750 and 1000 ms, and landing at 1200.
            (1_150, NOTE),
            (1_150, NOTE),
            (1_300, "Downloading"),
        ]
    );
}

/// Each file is fetched by a plan of its own, in order, from the one
/// repository at the one revision into the one directory.
#[tokio::test]
async fn each_file_has_a_plan_from_the_same_transfer() {
    let files = weights_and_projector();
    let destination = PathBuf::from("/models/owner/zeta-GGUF");
    let transfer = transfer(&destination);
    let plans = Mutex::new(Vec::new());

    let fetch_one = |plan: DownloadPlan<'_>| {
        plans.lock().unwrap().push((
            (plan.repo_id.to_string(), plan.revision.to_string()),
            plan.destination.to_path_buf(),
            (plan.file.to_string(), plan.expected_size),
            (plan.token.map(str::to_string), plan.force),
            plan.cancel.is_some(),
        ));
        async { Ok(()) }
    };
    let result = fetch_files(id(), &files, &transfer, None, fetch_one).await;

    assert_eq!(result, Ok(()));
    let from = ("owner/zeta-GGUF".to_string(), "2222222".to_string());
    let with = (Some("a-token".to_string()), true);
    assert_eq!(
        *plans.lock().unwrap(),
        [
            (
                from.clone(),
                destination.clone(),
                ("zeta.Q8_0.gguf".to_string(), Some(1_000)),
                with.clone(),
                false,
            ),
            (
                from,
                destination.clone(),
                ("mmproj-F16.gguf".to_string(), Some(300)),
                with,
                false,
            ),
        ]
    );
}

/// With no row to show a note on, a plan has no sink for notes, so the
/// accelerator's setup prints them, and it still has one for progress.
#[tokio::test]
async fn with_no_row_a_plan_has_no_sink_for_notes() {
    let files = weights_and_projector();
    let destination = PathBuf::from("/models/owner/zeta-GGUF");
    let transfer = transfer(&destination);
    let sinks = Mutex::new(Vec::new());

    let fetch_one = |plan: DownloadPlan<'_>| {
        sinks
            .lock()
            .unwrap()
            .push((plan.progress.is_some(), plan.notice.is_some()));
        async { Ok(()) }
    };
    let result = fetch_files(id(), &files, &transfer, None, fetch_one).await;

    assert_eq!(result, Ok(()));
    assert_eq!(*sinks.lock().unwrap(), [(true, false), (true, false)]);
}
