//! A [`DownloadManagerPort`] that fetches nothing and keeps what it was asked
//! to queue, for tests of what queues a download.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;

use super::DownloadManagerPort;
use crate::download::{DownloadError, DownloadId, QueueSnapshot};

/// Keeps each download it is asked to queue, as `(repository, quantization)`.
///
/// It answers the ID of the download as asked for, `repository:quantization`,
/// or refuses every request when it was made with [`Self::refusing`]. A
/// queue request is all it answers.
#[derive(Debug, Default)]
pub struct AskedDownloads {
    asked: Mutex<Vec<(String, Option<String>)>>,
    refusal: Option<DownloadError>,
}

impl AskedDownloads {
    /// A manager that refuses every queue request with `error`.
    #[must_use]
    pub fn refusing(error: DownloadError) -> Self {
        Self {
            refusal: Some(error),
            ..Self::default()
        }
    }

    /// Every download asked for so far, oldest first, refused or not.
    #[must_use]
    pub fn asked(&self) -> Vec<(String, Option<String>)> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait]
impl DownloadManagerPort for AskedDownloads {
    async fn queue_smart(
        self: Arc<Self>,
        repo_id: String,
        quantization: Option<String>,
    ) -> Result<DownloadId, DownloadError> {
        let id = DownloadId::new(repo_id.as_str(), quantization.as_deref());
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((repo_id, quantization));
        self.refusal.clone().map_or(Ok(id), Err)
    }

    async fn get_queue_snapshot(&self) -> Result<QueueSnapshot, DownloadError> {
        unimplemented!("a queue request is all this manager answers")
    }
    async fn cancel_download(&self, _id: &DownloadId) -> Result<(), DownloadError> {
        unimplemented!("a queue request is all this manager answers")
    }
    async fn cancel_all(&self) -> Result<(), DownloadError> {
        unimplemented!("a queue request is all this manager answers")
    }
    async fn active_count(&self) -> Result<u32, DownloadError> {
        unimplemented!("a queue request is all this manager answers")
    }
    async fn remove_from_queue(&self, _id: &DownloadId) -> Result<(), DownloadError> {
        unimplemented!("a queue request is all this manager answers")
    }
    async fn reorder_queue(&self, _id: &DownloadId, _position: u32) -> Result<u32, DownloadError> {
        unimplemented!("a queue request is all this manager answers")
    }
    async fn set_max_queue_size(&self, _size: u32) -> Result<(), DownloadError> {
        unimplemented!("a queue request is all this manager answers")
    }
}
