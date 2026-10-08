//! How [`fetch`] carries one file's count from transport to transport.

use std::future::{Ready, ready};
use std::sync::Mutex;

use super::native::tests::{Reply, TestServer, body_of};
use super::*;

type Seen = Arc<Mutex<Vec<FileProgress>>>;

/// How the stand-in accelerators of these tests end.
type Outcome = Ready<Result<(), PythonBridgeError>>;

/// What a call with no accelerator passes for one.
const NO_ACCELERATOR: Option<fn(RawCallback) -> Outcome> = None;

/// An address nothing is fetched from.
const NO_URL: &str = "http://unused.invalid/model.gguf";

/// A plan for `model.gguf` in `dir`, and everything it reports.
fn plan(dir: &Path, expected_size: Option<u64>) -> (DownloadPlan<'_>, Seen) {
    let seen = Seen::default();
    let sink = Arc::clone(&seen);
    let plan = DownloadPlan {
        repo_id: "owner/repo",
        revision: "main",
        destination: dir,
        file: "model.gguf",
        token: None,
        force: false,
        progress: Some(Arc::new(move |p| sink.lock().unwrap().push(p))),
        notice: None,
        expected_size,
        cancel: None,
    };
    (plan, seen)
}

const fn progress(bytes: u64, wire: u64, size: u64) -> FileProgress {
    FileProgress {
        bytes,
        wire,
        size: Some(size),
    }
}

/// A file found on disk is reported complete at its length, and none of
/// it as received: nothing came off the network.
#[tokio::test]
async fn a_file_already_on_disk_is_complete_with_nothing_received() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("model.gguf"), [0_u8; 640]).unwrap();
    let (plan, seen) = plan(dir.path(), None);

    fetch(&plan, NO_ACCELERATOR, NO_URL)
        .await
        .expect("nothing to fetch");

    assert_eq!(*seen.lock().unwrap(), [progress(640, 0, 640)]);
}

/// The accelerator has written every byte, and the file is complete only
/// once it has returned and the file is in place.
#[tokio::test]
async fn an_accelerated_file_is_complete_at_its_length_on_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let dest = dir.path().join("model.gguf");
    let (plan, seen) = plan(dir.path(), Some(640));
    let accelerator = |report: RawCallback| {
        std::fs::write(&dest, [0_u8; 640]).unwrap();
        report(RawProgress::new(640, 600, Some(640)));
        ready(Ok(()))
    };

    fetch(&plan, Some(accelerator), NO_URL)
        .await
        .expect("the accelerator fetched it");

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen, [progress(639, 600, 640), progress(640, 600, 640)]);
}

/// The accelerator fails part-way and the native transport fetches the file
/// from the start: the bytes on disk start again, and the bytes received
/// carry on from what the accelerator had received.
#[tokio::test]
async fn a_fallback_rewinds_the_bytes_and_continues_the_wire() {
    let body = body_of(4096);
    let server = TestServer::start(vec![Reply::File {
        body: body.clone(),
        etag: None,
    }])
    .await;
    let dir = tempfile::tempdir().expect("tempdir");
    let (plan, seen) = plan(dir.path(), Some(4096));
    let accelerator = |report: RawCallback| {
        report(RawProgress::new(800, 1000, None));
        ready(Err(PythonBridgeError::ProcessFailed("gone".to_string())))
    };

    fetch(&plan, Some(accelerator), &server.url())
        .await
        .expect("the native transport fetched it");

    let seen = seen.lock().unwrap().clone();
    assert_eq!(seen[0], progress(800, 1000, 4096));
    assert_eq!(seen[1], progress(0, 1000, 4096), "the one rewind");
    assert_eq!(seen[seen.len() - 1], progress(4096, 5096, 4096));
    assert_eq!(std::fs::read(dir.path().join("model.gguf")).unwrap(), body);
}
