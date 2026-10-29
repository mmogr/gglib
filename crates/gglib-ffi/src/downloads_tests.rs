//! A download through the library, from a fake of the Hub's file addresses
//! into a models folder of the test's: its progress, a cancel, a resume from
//! what a cancel kept, a pause and a resume, the model joining the library,
//! and its removal.

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gglib_core::ports::HfPortError;
use tempfile::TempDir;

use crate::download_feed::Standing;
use crate::library::Library;
use crate::test_gguf::chat_model;
use crate::test_hub::{HubFile, TestHub};
use crate::test_library::{dirs_in, library_over, wait};
use crate::test_server::FileServer;
use crate::{DownloadProgress, DownloadState, DownloadWatch, GglibError};

const REPO: &str = "tester/tiny-chat-GGUF";
const FILE: &str = "tiny-chat-Q4_K_M.gguf";
const QUANT: &str = "Q4_K_M";
const MIB: usize = 1 << 20;
/// The file's size: a chat model's header, then bytes that stand in for
/// its tensors.
const SIZE: usize = 3 * MIB;

/// A test's library, the file server its Hub sends downloads to, and the
/// file it serves.
struct Rig {
    root: TempDir,
    server: FileServer,
    library: Arc<Library>,
    bytes: Vec<u8>,
}

fn rig() -> Rig {
    let mut bytes = chat_model("llama", 40_960).bytes();
    bytes.extend((bytes.len()..SIZE).map(|i| u8::try_from(i % 251).unwrap()));
    let file = HubFile {
        repo: REPO.to_owned(),
        path: FILE.to_owned(),
        bytes: bytes.clone(),
    };
    let server = FileServer::start(std::slice::from_ref(&file));
    let hub = TestHub {
        files: vec![file],
        file_server: Some(server.url.clone()),
        ..TestHub::default()
    };
    let root = TempDir::new().unwrap();
    let library = library_over(hub, &root);
    Rig {
        root,
        server,
        library,
        bytes,
    }
}

impl Rig {
    /// Where the downloaded file lands.
    fn landed(&self) -> PathBuf {
        PathBuf::from(dirs_in(&self.root).models_dir)
            .join("tester_tiny-chat-GGUF")
            .join(FILE)
    }

    fn part(&self) -> PathBuf {
        self.landed().with_extension("gguf.part")
    }

    fn download(&self) -> Arc<DownloadWatch> {
        within(Arc::clone(&self.library).download(REPO.to_owned(), QUANT.to_owned()))
            .expect("the download is queued")
    }

    /// Where the feed has the download.
    fn standing(&self) -> Option<Standing> {
        let tracked = self.library.feed.tracked(&format!("{REPO}:{QUANT}"));
        tracked.map(|tracked| tracked.standing)
    }

    /// Wait for the download manager to have ended the download cancelled.
    fn until_cancelled(&self) {
        within(async {
            while self.standing() != Some(Standing::Cancelled) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
    }
}

/// Run `call` to its end, failing the test past a minute.
fn within<F: Future>(call: F) -> F::Output {
    wait(async {
        tokio::time::timeout(Duration::from_mins(1), call)
            .await
            .expect("the call ended within a minute")
    })
}

/// Every answer of `watch` until it is over.
fn follow(watch: &Arc<DownloadWatch>) -> Vec<DownloadProgress> {
    std::iter::from_fn(|| within(Arc::clone(watch).next())).collect()
}

/// The answers of `watch` up to the first that `stop` holds of.
fn follow_until(
    watch: &Arc<DownloadWatch>,
    stop: impl Fn(&DownloadProgress) -> bool,
) -> Vec<DownloadProgress> {
    let mut answers = Vec::new();
    while let Some(progress) = within(Arc::clone(watch).next()) {
        let done = stop(&progress);
        answers.push(progress);
        if done {
            return answers;
        }
    }
    panic!("the watch ended first: {answers:?}");
}

fn running_with_bytes(progress: &DownloadProgress) -> bool {
    progress.state == DownloadState::Running && progress.bytes > 0
}

/// Each answer's bytes, in order.
fn bytes_of(answers: &[DownloadProgress]) -> Vec<u64> {
    answers.iter().map(|progress| progress.bytes).collect()
}

#[test]
fn a_download_fills_its_bar_lands_in_the_models_folder_and_joins_the_library() {
    let rig = rig();

    let answers = follow(&rig.download());

    let last = answers.last().expect("the watch answered");
    assert_eq!(last.state, DownloadState::Done, "{answers:?}");
    assert_eq!((last.bytes, last.total), (SIZE as u64, Some(SIZE as u64)));
    assert!(bytes_of(&answers).is_sorted(), "{answers:?}");
    assert_eq!(std::fs::read(rig.landed()).unwrap(), rig.bytes);
    assert!(!rig.part().exists());
    let installed = within(Arc::clone(&rig.library).installed()).unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(
        (installed[0].bytes, installed[0].trained_ctx),
        (SIZE as u64, Some(40_960))
    );
}

/// A cancel keeps what was fetched as the file's `.part`, and the next
/// download of it asks only for the rest.
#[test]
fn a_cancelled_download_keeps_what_it_fetched_and_the_next_carries_on_from_it() {
    let rig = rig();
    rig.server.stall_once(MIB);
    let first = rig.download();
    follow_until(&first, running_with_bytes);

    first.cancel();

    assert_eq!(within(Arc::clone(&first).next()), None);
    rig.until_cancelled();
    let kept = std::fs::metadata(rig.part())
        .expect("the part is kept")
        .len();
    assert!(kept > 0 && kept <= MIB as u64, "{kept}");

    let answers = follow(&rig.download());

    assert_eq!(
        answers.last().map(|progress| &progress.state),
        Some(&DownloadState::Done)
    );
    assert!(
        rig.server
            .asked()
            .iter()
            .any(|(_, range)| range.as_deref() == Some(&format!("bytes={kept}-"))),
        "{:?}",
        rig.server.asked()
    );
    assert_eq!(std::fs::read(rig.landed()).unwrap(), rig.bytes);
    assert_eq!(
        within(Arc::clone(&rig.library).installed()).unwrap().len(),
        1
    );
    rig.server.release();
}

/// A pause stops the download and keeps its watch; a resume carries it on
/// to the end from what it fetched.
#[test]
fn a_paused_download_is_carried_on_by_a_resume() {
    let rig = rig();
    rig.server.stall_once(MIB);
    let watch = rig.download();
    follow_until(&watch, running_with_bytes);

    within(Arc::clone(&rig.library).pause_all());
    follow_until(&watch, |progress| progress.state == DownloadState::Paused);
    within(Arc::clone(&rig.library).resume_all());
    let answers = follow(&watch);

    assert_eq!(
        answers.last().map(|progress| &progress.state),
        Some(&DownloadState::Done)
    );
    assert!(
        rig.server.asked().iter().any(|(_, range)| range.is_some()),
        "the resume asked for the rest: {:?}",
        rig.server.asked()
    );
    assert_eq!(std::fs::read(rig.landed()).unwrap(), rig.bytes);
    rig.server.release();
}

/// A pause returns once the download has stopped, so a resume straight
/// after it, the watch not read in between, queues it again rather than
/// finding the cancelled transfer still running.
#[test]
fn a_resume_straight_after_a_pause_carries_the_download_on() {
    let rig = rig();
    rig.server.stall_once(MIB);
    let watch = rig.download();
    follow_until(&watch, running_with_bytes);

    within(Arc::clone(&rig.library).pause_all());
    let paused = rig.standing();
    within(Arc::clone(&rig.library).resume_all());
    let answers = follow(&watch);

    let last = answers.last().map(|progress| &progress.state);
    assert_eq!(last, Some(&DownloadState::Done), "{answers:?}");
    assert!(rig.server.asked().iter().any(|(_, range)| range.is_some()));
    let paused = matches!(paused, Some(Standing::At(at)) if at.state == DownloadState::Paused);
    assert!(paused, "the pause returned before the download stopped");
    rig.server.release();
}

/// The digest the Hub gives is checked: bytes that do not match it fail the
/// download, and nothing of them is kept.
#[test]
fn a_file_that_does_not_match_its_digest_fails_and_is_not_kept() {
    let rig = rig();
    rig.server.lie_about_digests();

    let answers = follow(&rig.download());

    let last = answers.last().expect("the watch answered");
    assert!(
        matches!(&last.state, DownloadState::Failed { reason } if reason.contains("Integrity")),
        "{answers:?}"
    );
    assert!(!rig.landed().exists() && !rig.part().exists());
    assert_eq!(within(Arc::clone(&rig.library).installed()), Ok(Vec::new()));
}

#[test]
fn a_downloaded_model_is_removed_with_its_folder() {
    let rig = rig();
    follow(&rig.download());
    let installed = within(Arc::clone(&rig.library).installed()).unwrap();

    within(Arc::clone(&rig.library).remove(installed[0].id)).expect("removed");

    assert!(!rig.landed().exists());
    assert!(!rig.landed().parent().unwrap().exists());
    assert_eq!(within(Arc::clone(&rig.library).installed()), Ok(Vec::new()));
}

#[test]
fn a_download_the_hub_refuses_is_never_queued() {
    let root = TempDir::new().unwrap();
    let hub = TestHub {
        refusal: Some(|| HfPortError::AuthRequired {
            model_id: REPO.to_owned(),
        }),
        ..TestHub::default()
    };
    let library = library_over(hub, &root);

    let refused = within(library.download(REPO.to_owned(), QUANT.to_owned()));

    assert_eq!(refused.err(), Some(GglibError::AuthRequired));
}

#[test]
fn a_quantization_the_repository_lacks_is_not_found() {
    let rig = rig();

    let missing = within(Arc::clone(&rig.library).download(REPO.to_owned(), "Q8_0".to_owned()));

    assert_eq!(missing.err(), Some(GglibError::NotFound));
    assert!(rig.server.asked().is_empty());
}
