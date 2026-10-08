//! A download queued again starts from nothing.

use super::super::duplicate_guard_tests::NoRegistrar;
use super::super::test_support::{End, run_next};
use super::super::*;
use super::tests::{ID, REPO, queued};

/// Queue the fixture's download again and start its first file. Answers the
/// row it then has.
async fn queued_again(manager: &DownloadManagerImpl) -> gglib_core::download::DownloadRow {
    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .unwrap();
    manager.next_job().await.expect("the weights are pending");
    let snapshot = manager.get_queue_snapshot().await.unwrap();
    snapshot.active.expect("a running row")
}

/// The weights land and the projector fails. The weights' bytes belong to
/// the run that failed: the retry counts from 0.
#[tokio::test]
async fn a_retry_does_not_start_with_the_failed_runs_bytes() {
    let f = queued(Arc::new(NoRegistrar)).await;
    assert_eq!(run_next(&f.manager, End::OnDisk).await, "zeta.Q8_0.gguf");
    assert_eq!(f.manager.meters().len(), 1, "the meter the weights fed");
    assert_eq!(run_next(&f.manager, End::Failed).await, "mmproj-F16.gguf");

    let row = queued_again(&f.manager).await;

    assert_eq!(row.id, ID);
    assert_eq!((row.downloaded_bytes, row.total_bytes), (0, Some(1_300)));
    assert_eq!(row.text.percent, "0.0%");
}

/// The weights land and the download is cancelled before its projector
/// starts. Queued again, it does not start with the weights' bytes.
#[tokio::test]
async fn a_download_cancelled_between_files_is_queued_again_from_nothing() {
    let f = queued(Arc::new(NoRegistrar)).await;
    let id = DownloadId::new(REPO, Some("Q8_0"));
    run_next(&f.manager, End::OnDisk).await;
    f.manager.cancel_download(&id).await.unwrap();
    assert!(f.manager.get_queue_snapshot().await.unwrap().is_idle());

    let row = queued_again(&f.manager).await;

    assert_eq!((row.downloaded_bytes, row.total_bytes), (0, Some(1_300)));
}
