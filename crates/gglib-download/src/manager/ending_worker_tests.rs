//! A download ends as a whole when its worker's file ends it: a file that
//! failed, a cancel that the worker's answer cannot undo, a cancel that came
//! too late, and a model the library refused.

use std::sync::{Mutex as StdMutex, OnceLock};

use async_trait::async_trait;
use gglib_core::RepositoryError;
use gglib_core::ports::{CompletedDownload, ModelRegistrarPort, RegisteredDownload};

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::group_registration_tests::RecordingRegistrar;
use super::super::test_support::{End, end_started, run_next, start_next};
use super::super::*;
use super::ending_tests::{finished, id, summaries};
use super::tests::{ID, failed, queued};

/// A registrar the database refuses.
struct FailingRegistrar;

#[async_trait]
impl ModelRegistrarPort for FailingRegistrar {
    async fn register_model(
        &self,
        _download: &CompletedDownload,
    ) -> Result<RegisteredDownload, RepositoryError> {
        Err(RepositoryError::Storage("disk full".to_string()))
    }
}

/// A registrar that cancels the download it is asked to register, as a user
/// might while the row reads "Registering", and then takes the model.
#[derive(Default)]
struct CancelsWhileRegistering {
    manager: OnceLock<Arc<DownloadManagerImpl>>,
    /// Whether the cancel was answered `Ok`.
    cancel_accepted: StdMutex<Option<bool>>,
    library: RecordingRegistrar,
}

#[async_trait]
impl ModelRegistrarPort for CancelsWhileRegistering {
    async fn register_model(
        &self,
        download: &CompletedDownload,
    ) -> Result<RegisteredDownload, RepositoryError> {
        let manager = self.manager.get().expect("the test gave the manager");
        let answer = manager.cancel_download(&id()).await;
        *self.cancel_accepted.lock().unwrap() = Some(answer.is_ok());
        self.library.register_model(download).await
    }
}

/// The weights fail. The projector is never fetched, no row is left, and
/// the queue drains with one failed download.
#[tokio::test]
async fn a_failed_file_ends_its_download() {
    let f = queued(Arc::new(NoRegistrar)).await;

    assert_eq!(run_next(&f.manager, End::Failed).await, "zeta.Q8_0.gguf");

    assert!(f.manager.next_job().await.is_none(), "one file was fetched");
    let last = f.recorded.snapshots().pop().unwrap();
    assert!(last.is_idle(), "{:?}", last.rows().collect::<Vec<_>>());
    assert_eq!(
        finished(&f),
        [(ID.to_string(), failed("Network error: connection reset"))]
    );
    let [summary] = &summaries(&f)[..] else {
        panic!("the queue drained once, not {:?}", summaries(&f).len());
    };
    assert_eq!(summary.unique_models_failed, 1);
}

/// The weights land and the projector fails. The group the weights opened
/// is closed with the download, so the queue is drained and says so.
#[tokio::test]
async fn a_failure_after_a_file_landed_closes_the_group_and_drains() {
    let f = queued(Arc::new(NoRegistrar)).await;
    run_next(&f.manager, End::OnDisk).await;
    assert!(f.manager.shard_tracker.lock().await.has_open_groups());

    run_next(&f.manager, End::Failed).await;

    assert!(!f.manager.shard_tracker.lock().await.has_open_groups());
    assert!(f.manager.meters().is_empty());
    assert_eq!(summaries(&f).len(), 1, "the drained queue is summed up");
}

/// The download is cancelled as its last file lands. Cancel wins: the model
/// is not registered, the download is cancelled, and its group stays closed
/// to a file landing later still.
#[tokio::test]
async fn a_cancel_after_ok_is_cancelled_and_the_group_stays_closed() {
    let registrar = Arc::new(RecordingRegistrar::default());
    let f = queued(registrar.clone()).await;
    run_next(&f.manager, End::OnDisk).await;
    let projector = start_next(&f.manager).await;
    let group = projector.item.group_id.clone().expect("a group");

    f.manager.cancel_download(&id()).await.unwrap();
    end_started(&f.manager, projector, End::OnDisk).await;

    assert!(registrar.registered.lock().unwrap().is_empty());
    assert_eq!(finished(&f), [(ID.to_string(), DownloadOutcome::Cancelled)]);
    let metadata = GroupMetadata {
        repo_id: "owner/zeta-GGUF".to_string(),
        commit_sha: "abc123".to_string(),
        quantization: gglib_core::download::Quantization::Q8_0,
        primary_filename: "zeta.Q8_0.gguf".to_string(),
        hf_tags: vec![],
        file_entries: vec![],
    };
    let mut tracker = f.manager.shard_tracker.lock().await;
    let open_at_the_end = tracker.has_open_groups();
    let late = tracker.on_shard_done(&group, 1, "mmproj-F16.gguf".into(), 2, &metadata);
    let open_after_a_late_file = tracker.has_open_groups();
    drop(tracker);
    assert!(!open_at_the_end);
    assert!(late.is_none());
    assert!(!open_after_a_late_file, "a late file does not reopen it");
}

/// The cancel arrives while the last file's model is being registered. It
/// is accepted and it is too late: the model is in the library, so the
/// download ended completed, and not cancelled.
#[tokio::test]
async fn a_cancel_while_the_model_is_registered_is_too_late() {
    let registrar = Arc::new(CancelsWhileRegistering::default());
    let f = queued(registrar.clone()).await;
    assert!(registrar.manager.set(Arc::clone(&f.manager)).is_ok());

    run_next(&f.manager, End::OnDisk).await;
    run_next(&f.manager, End::OnDisk).await;

    assert_eq!(*registrar.cancel_accepted.lock().unwrap(), Some(true));
    assert_eq!(registrar.library.registered.lock().unwrap().len(), 1);
    let ended = finished(&f);
    assert!(
        matches!(&ended[..], [(id, DownloadOutcome::Completed { message: Some(_) })] if id == ID),
        "{ended:?}"
    );
}

/// A worker that answers with an error after it was told to stop was
/// cancelled, not failed.
#[tokio::test]
async fn a_cancelled_download_is_cancelled_whatever_its_worker_answers() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let weights = start_next(&f.manager).await;

    f.manager.cancel_download(&id()).await.unwrap();
    end_started(&f.manager, weights, End::Failed).await;

    assert_eq!(finished(&f), [(ID.to_string(), DownloadOutcome::Cancelled)]);
}

/// The cancel arrives after the weights were counted toward the group and
/// before they leave `active`. The download ends there, cancelled, and is
/// not left between two files with its projector waiting.
#[tokio::test]
async fn a_cancel_while_a_file_is_put_away_ends_the_download() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let weights = start_next(&f.manager).await;

    f.manager.cancel_download(&id()).await.unwrap();
    f.manager.settle(&weights.item.id, None).await;

    assert_eq!(finished(&f), [(ID.to_string(), DownloadOutcome::Cancelled)]);
    assert_eq!(f.manager.queue.read().await.pending_len(), 0);
    assert!(f.recorded.snapshots().pop().unwrap().is_idle());
}

/// A file with more to come and no cancel ends nothing: the download goes
/// on to its next file.
#[tokio::test]
async fn a_file_with_more_to_come_ends_nothing() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let weights = start_next(&f.manager).await;

    f.manager.settle(&weights.item.id, None).await;

    assert!(finished(&f).is_empty());
    assert_eq!(f.manager.queue.read().await.pending_len(), 1);
}

/// Every file landed and the library refused the model. The run's summary
/// counts a failed download, not a downloaded one.
#[tokio::test]
async fn run_summary_counts_a_registration_failure_as_failed() {
    let f = queued(Arc::new(FailingRegistrar)).await;

    run_next(&f.manager, End::OnDisk).await;
    run_next(&f.manager, End::OnDisk).await;

    let [summary] = &summaries(&f)[..] else {
        panic!("the queue drained once, not {:?}", summaries(&f).len());
    };
    assert_eq!(
        (
            summary.unique_models_failed,
            summary.unique_models_downloaded,
            summary.total_attempts_failed,
            summary.total_attempts_downloaded,
        ),
        (1, 0, 1, 0)
    );
}

/// A model that registered is counted as downloaded, once.
#[tokio::test]
async fn run_summary_counts_a_registered_model_as_downloaded() {
    let f = queued(Arc::new(RecordingRegistrar::default())).await;

    run_next(&f.manager, End::OnDisk).await;
    run_next(&f.manager, End::OnDisk).await;

    let [summary] = &summaries(&f)[..] else {
        panic!("the queue drained once, not {:?}", summaries(&f).len());
    };
    assert_eq!(
        (
            summary.unique_models_downloaded,
            summary.total_attempts_downloaded,
            summary.unique_models_failed,
        ),
        (1, 1, 0)
    );
}

/// The file leaves `active` and the outcome joins the finished list under
/// one queue guard. A reader waiting for the queue when the ending began
/// gets it next, and must not find the download in neither place.
#[tokio::test]
async fn the_outcome_is_recorded_under_the_guard_that_empties_active() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let weights = start_next(&f.manager).await;
    let download = weights.item.id.clone();

    // The ending takes the queue and then waits here for `active`.
    let active = f.manager.active.lock().await;
    let manager = Arc::clone(&f.manager);
    let ending = tokio::spawn({
        let download = download.clone();
        async move {
            let outcome = failed("connection reset");
            manager.settle(&download, Some(outcome)).await;
        }
    });
    tokio::task::yield_now().await;
    assert!(
        f.manager.queue.try_read().is_err(),
        "the ending has the queue"
    );

    // The reader queues behind it for the queue.
    let manager = Arc::clone(&f.manager);
    let reader = tokio::spawn(async move {
        let queue = manager.queue.read().await;
        let is_active = manager.active.lock().await.contains_key(&download);
        let is_finished = queue.finished().iter().any(|ended| ended.id == ID);
        drop(queue);
        (is_active, is_finished)
    });
    tokio::task::yield_now().await;
    drop(active);

    let (is_active, is_finished) = reader.await.unwrap();
    ending.await.unwrap();
    assert!(!is_active);
    assert!(is_finished, "gone from `active` with no outcome recorded");
}
