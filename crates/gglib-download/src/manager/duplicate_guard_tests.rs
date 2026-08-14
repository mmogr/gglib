//! Tests for the duplicate-enqueue guard in `queue_download_smart`.
//!
//! Kept out of `mod.rs`: the port stubs a real `DownloadManagerImpl` needs are
//! bulky and self-contained, and that file is already well over the size
//! ratchet's budget.

use super::*;
use async_trait::async_trait;
use gglib_core::download::{DownloadStatus, QueuedDownload};
use gglib_core::ports::huggingface::{
    HfClientPort, HfFileInfo, HfPortResult, HfQuantInfo, HfRepoInfo, HfSearchOptions,
    HfSearchResult,
};
use gglib_core::ports::{
    CompletedDownload, DownloadStateRepositoryPort, ModelRegistrarPort, NoopDownloadEmitter,
};
use gglib_core::{Model, RepositoryError};

const REPO: &str = "owner/repo";

/// An HF repo with exactly one single-file `Q8_0` quantization.
struct OneQuantHf;

#[async_trait]
impl HfClientPort for OneQuantHf {
    async fn list_quantizations(&self, _model_id: &str) -> HfPortResult<Vec<HfQuantInfo>> {
        Ok(vec![HfQuantInfo {
            name: "Q8_0".to_string(),
            shard_count: 1,
            total_size: 1_024,
            file_paths: vec!["model-Q8_0.gguf".to_string()],
        }])
    }

    async fn get_quantization_files(
        &self,
        _model_id: &str,
        _quantization: &str,
    ) -> HfPortResult<Vec<HfFileInfo>> {
        Ok(vec![HfFileInfo {
            path: "model-Q8_0.gguf".to_string(),
            size: 1_024,
            is_gguf: true,
            oid: None,
        }])
    }

    async fn search(&self, _options: &HfSearchOptions) -> HfPortResult<HfSearchResult> {
        unimplemented!("not reached by queue_download_smart")
    }
    async fn list_gguf_files(&self, _model_id: &str) -> HfPortResult<Vec<HfFileInfo>> {
        unimplemented!("not reached by queue_download_smart")
    }
    async fn get_commit_sha(&self, _model_id: &str) -> HfPortResult<String> {
        unimplemented!("not reached by queue_download_smart")
    }
    async fn get_model_info(&self, _model_id: &str) -> HfPortResult<HfRepoInfo> {
        unimplemented!("not reached by queue_download_smart")
    }
}

struct NoRegistrar;

#[async_trait]
impl ModelRegistrarPort for NoRegistrar {
    async fn register_model(
        &self,
        _download: &CompletedDownload,
    ) -> Result<Model, RepositoryError> {
        unimplemented!("nothing completes in these tests")
    }
}

struct NoRepo;

#[async_trait]
impl DownloadStateRepositoryPort for NoRepo {
    async fn enqueue(&self, _download: &QueuedDownload) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn update_status(
        &self,
        _id: &DownloadId,
        _status: DownloadStatus,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn load_queue(&self) -> Result<Vec<QueuedDownload>, RepositoryError> {
        Ok(Vec::new())
    }
    async fn mark_failed(
        &self,
        _id: &DownloadId,
        _error_message: &str,
    ) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn remove(&self, _id: &DownloadId) -> Result<(), RepositoryError> {
        Ok(())
    }
    async fn prune_completed(&self, _older_than_days: u32) -> Result<u32, RepositoryError> {
        Ok(0)
    }
}

fn test_manager() -> DownloadManagerImpl {
    DownloadManagerImpl::new(
        Arc::new(NoRegistrar),
        Arc::new(NoRepo),
        Arc::new(OneQuantHf),
        Arc::new(NoopDownloadEmitter::new()),
        DownloadManagerConfig::default(),
    )
}

/// Move the head of the pending queue into `active`, as the runner does.
async fn start_head(manager: &DownloadManagerImpl) -> DownloadId {
    let item = manager
        .queue
        .write()
        .await
        .dequeue()
        .expect("something must be pending");
    let id = item.id.clone();
    let (progress_tx, _rx) = watch::channel(ProgressUpdate::new(0, 0, 0));
    manager.active.lock().await.insert(
        id.clone(),
        ActiveJob {
            lease: LeaseId(1),
            cancel: CancellationToken::new(),
            progress_tx,
            shard_info: None,
            group_id: None,
        },
    );
    id
}

/// The reported bug: the first request had already started downloading, so
/// `Queue::is_queued` (pending only) stopped matching and every retry of
/// `gglib model download` appended another copy of the same model.
#[tokio::test]
async fn repeat_request_while_active_attaches_instead_of_duplicating() {
    let manager = test_manager();

    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .expect("first queue");
    let active_id = start_head(&manager).await;
    assert_eq!(manager.queue.read().await.pending_len(), 0);

    let again = manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .expect("a repeat request attaches rather than failing");

    assert_eq!(again.root_id, active_id, "must attach to the live download");
    assert_eq!(
        manager.queue.read().await.pending_len(),
        0,
        "the repeat request must not append a second entry"
    );
}

/// The pending case was already guarded by `check_not_queued`, which failed
/// the call. It now attaches too, so re-running the command is uniformly
/// safe rather than depending on whether the download had started yet.
#[tokio::test]
async fn repeat_request_while_pending_attaches_instead_of_duplicating() {
    let manager = test_manager();

    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .expect("first queue");
    assert_eq!(manager.queue.read().await.pending_len(), 1);

    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .expect("a repeat request attaches rather than failing");

    assert_eq!(
        manager.queue.read().await.pending_len(),
        1,
        "the repeat request must not append a second entry"
    );
}

/// Why the guard checks `active` and `pending` by hand instead of calling
/// `has_download`: that also answers `true` for a *failed* download, which
/// would make a failure permanently un-retryable.
#[tokio::test]
async fn a_failed_download_can_still_be_requeued() {
    let manager = test_manager();

    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .expect("first queue");
    {
        let mut queue = manager.queue.write().await;
        let item = queue.dequeue().expect("something must be pending");
        queue.mark_failed(item, "network died");
    }
    assert_eq!(manager.queue.read().await.pending_len(), 0);

    manager
        .queue_download_smart(REPO, Some("Q8_0".to_string()))
        .await
        .expect("a failed download must stay re-queueable");

    assert_eq!(
        manager.queue.read().await.pending_len(),
        1,
        "retry after failure must enqueue again"
    );
}
