//! How a download's ending is recorded and announced.

use async_trait::async_trait;
use gglib_core::RepositoryError;
use gglib_core::ports::{CompletedDownload, ModelRegistrarPort, RegisteredDownload};

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::group_registration_tests::RecordingRegistrar;
use super::super::test_support::{End, run_next};
use super::super::*;
use super::tests::{ID, REPO, failed, queued};

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

/// A download that ends well is gone from the rows, in the finished list
/// with its message, and announced once.
#[tokio::test]
async fn a_completed_download_is_recorded_with_its_message() {
    let f = queued(Arc::new(RecordingRegistrar::default())).await;

    run_next(&f.manager, End::OnDisk).await;
    let between = f.manager.get_queue_snapshot().await.unwrap();
    assert!(between.finished.is_empty(), "a file is not the download");
    assert!(
        f.manager
            .meters()
            .contains_key(&DownloadId::new(REPO, Some("Q8_0")))
    );
    run_next(&f.manager, End::OnDisk).await;

    let last = f.recorded.snapshots().pop().unwrap();
    assert!(last.is_idle());
    let [ended] = &last.finished[..] else {
        panic!("one finished download, not {:?}", last.finished);
    };
    assert_eq!((ended.id.as_str(), ended.title.as_str()), (ID, ID));
    let DownloadOutcome::Completed {
        message: Some(message),
    } = &ended.outcome
    else {
        panic!("completed with a message, not {:?}", ended.outcome);
    };
    assert!(
        message.ends_with("with its projector mmproj-F16.gguf"),
        "{message}"
    );
    let endings = f.recorded.endings();
    assert!(
        matches!(&endings[..], [DownloadEvent::DownloadCompleted { id, message: Some(m) }] if id == ID && m == message),
        "{endings:?}"
    );
    assert!(
        f.manager.meters().is_empty(),
        "the meter goes with the download"
    );

    // The run's summary names the download as its row did.
    let named: Vec<String> = f
        .recorded
        .events()
        .into_iter()
        .filter_map(|event| match event {
            DownloadEvent::QueueRunComplete { summary } => Some(summary.items),
            _ => None,
        })
        .flatten()
        .map(|item| item.display_name)
        .collect();
    assert_eq!(named, [ID]);
}

/// The files are on disk and the library refused the model: that is a
/// failed download, on the snapshot and as an event, and not a completed
/// one.
#[tokio::test]
async fn a_registration_error_is_a_failed_outcome() {
    let f = queued(Arc::new(FailingRegistrar)).await;

    run_next(&f.manager, End::OnDisk).await;
    run_next(&f.manager, End::OnDisk).await;

    let served = f.manager.get_queue_snapshot().await.unwrap();
    assert!(served.is_idle());
    let [ended] = &served.finished[..] else {
        panic!("one finished download, not {:?}", served.finished);
    };
    assert_eq!(ended.id, ID);
    assert_eq!(
        ended.outcome,
        failed("Registration failed: Storage error: disk full")
    );
    let endings = f.recorded.endings();
    assert!(
        matches!(&endings[..], [DownloadEvent::DownloadFailed { id, error }] if id == ID && error.starts_with("Registration failed")),
        "{endings:?}"
    );
}

#[tokio::test]
async fn a_failed_file_is_a_failed_outcome_and_a_cancelled_one_cancelled() {
    for (end, outcome) in [
        (End::Failed, failed("Network error: connection reset")),
        (End::Cancelled, DownloadOutcome::Cancelled),
    ] {
        let f = queued(Arc::new(NoRegistrar)).await;

        run_next(&f.manager, end).await;

        let last = f.recorded.snapshots().pop().unwrap();
        let ended: Vec<_> = last
            .finished
            .iter()
            .map(|f| (f.id.as_str(), &f.outcome))
            .collect();
        assert_eq!(ended, [(ID, &outcome)]);
        let endings = f.recorded.endings();
        assert_eq!(endings.len(), 1, "{endings:?}");
        assert_eq!(endings[0].id(), Some(ID));
        assert_eq!(
            matches!(endings[0], DownloadEvent::DownloadCancelled { .. }),
            end == End::Cancelled
        );
        assert!(f.manager.meters().is_empty());
    }
}

/// Clearing the failures is published, so a client's list empties without
/// its asking again.
#[tokio::test]
async fn clearing_the_failures_publishes_the_shorter_list() {
    let f = queued(Arc::new(NoRegistrar)).await;
    run_next(&f.manager, End::Failed).await;
    assert_eq!(f.recorded.snapshots().pop().unwrap().finished.len(), 1);

    f.manager.clear_failed().await.unwrap();

    assert!(f.recorded.snapshots().pop().unwrap().finished.is_empty());
}
