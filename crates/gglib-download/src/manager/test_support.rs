//! What the manager's tests share: an emitter that keeps what it is sent,
//! and a stand-in for the worker.

use std::path::Path;
use std::sync::Mutex as StdMutex;
use std::time::Instant;

use gglib_core::download::{Quantization, QueueSnapshot};

use super::worker::CompletedJob;
use super::*;
use crate::executor::FileProgress;

/// Keeps every download event emitted.
#[derive(Default)]
pub(super) struct Recorded(StdMutex<Vec<DownloadEvent>>);

impl AppEventEmitter for Recorded {
    fn emit(&self, event: AppEvent) {
        if let AppEvent::Download { event } = event {
            self.0.lock().unwrap().push(event);
        }
    }
}

impl Recorded {
    pub(super) fn events(&self) -> Vec<DownloadEvent> {
        self.0.lock().unwrap().clone()
    }

    /// The snapshots sent, in the order they were.
    pub(super) fn snapshots(&self) -> Vec<QueueSnapshot> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                DownloadEvent::QueueSnapshot(snapshot) => Some(*snapshot),
                _ => None,
            })
            .collect()
    }

    /// The events that say a download ended, in the order they were sent.
    pub(super) fn endings(&self) -> Vec<DownloadEvent> {
        self.events()
            .into_iter()
            .filter(|event| event.id().is_some())
            .collect()
    }
}

/// How the worker's run of a file ends.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum End {
    /// Every byte on disk.
    OnDisk,
    /// The transfer failed.
    Failed,
    /// The user cancelled it.
    Cancelled,
}

/// A reading of `bytes` on disk, all of them received, of a file of `size`.
pub(super) const fn reading(bytes: u64, size: u64) -> ProgressUpdate {
    ProgressUpdate {
        progress: FileProgress {
            bytes,
            wire: bytes,
            size: Some(size),
        },
        notice: None,
    }
}

/// The size the queue has for `item`'s file.
pub(super) fn size_of(item: &QueuedItem) -> u64 {
    item.shard_info
        .as_ref()
        .and_then(|place| place.file_size)
        .unwrap_or(0)
}

/// A file started as the runner starts it, and not yet ended.
pub(super) struct Started {
    pub(super) lease: LeaseId,
    pub(super) item: QueuedItem,
    pub(super) cancel: CancellationToken,
}

/// Start the next file as the runner does.
pub(super) async fn start_next(manager: &DownloadManagerImpl) -> Started {
    let (lease, item, cancel, _progress) = manager.next_job().await.expect("a file is pending");
    Started {
        lease,
        item,
        cancel,
    }
}

/// Start the next file as the runner does, and end it as the worker would.
/// Answers the file's name.
pub(super) async fn run_next(manager: &DownloadManagerImpl, end: End) -> String {
    let started = start_next(manager).await;
    end_started(manager, started, end).await
}

/// End a started file as the worker would: its bytes read by the meter when
/// it is on disk, and then finalized. Answers the file's name.
pub(super) async fn end_started(
    manager: &DownloadManagerImpl,
    started: Started,
    end: End,
) -> String {
    let Started { lease, item, .. } = started;
    let name = item.shard_info.as_ref().unwrap().filename.clone();
    let path = Path::new("models").join(&name);
    let result = match end {
        End::OnDisk => {
            let size = size_of(&item);
            manager.observe(&item.id, &reading(size, size), Instant::now());
            Ok(CompletedJob {
                primary_path: path.clone(),
                all_paths: vec![path],
                repo_id: item.id.model_id().to_string(),
                commit_sha: "abc123".to_string(),
                quantization: Quantization::Q8_0,
                files: vec![name.clone()],
            })
        }
        End::Failed => Err(DownloadError::network("connection reset")),
        End::Cancelled => Err(DownloadError::Cancelled),
    };
    manager.finalize_job(&item, lease, result).await;
    name
}
